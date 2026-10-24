//! The value-algebra differential gate: two captured revisions compared
//! over a persistent, append-only witness universe under an explicit
//! construction policy, with hash-chained receipts.
//!
//! [`capture`] verifies each revision's producer dump against its own
//! manifest. The comparison itself is [`crate::acceptance`]'s policy-free
//! core; this module adds witness identity, history and policy.

pub mod capture;
pub mod commands;
pub mod compare;
pub mod history;

/// Why a differential-gate command could not run to a verdict.
///
/// The command-line tool maps [`DiffError::Request`] and
/// [`DiffError::Identity`] to exit code 2 and [`DiffError::Infrastructure`]
/// to exit code 3; semantic failures are outcomes, not errors.
#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    /// The request is malformed or names something that does not exist.
    #[error("invalid request: {0}")]
    Request(String),
    /// An input's identity, provenance or history chain is wrong.
    #[error("identity: {0}")]
    Identity(String),
    /// A tool, process, file or coverage failure.
    #[error("infrastructure: {0}")]
    Infrastructure(String),
}

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::acceptance::discovery::{read_coalesced_defaults, validate_probe_coverage};
use crate::acceptance::pool::PoolLimits;
use crate::acceptance::screen::ImportedRoster;
use crate::acceptance::{AcceptanceComparison, ComparisonConfig, compare_charts};
use crate::helm::adjudication::OfflineKubernetesValidator;
use capture::{ArtifactPair, DumpPair, chart_keys};
use compare::{ChartIdentity, Class, OutcomeTable, classify, replay_witness, witness_record};
use history::{BlobStore, UniverseState, WitnessRecord, canonical_json};

/// What one dump-pair comparison needs besides the pair.
pub struct RunOptions {
    /// Live adjudication's validator; without it no transition is decided.
    pub kubernetes: Option<OfflineKubernetesValidator>,
    /// The context roster for exactly the selected charts.
    pub contexts: Option<ImportedRoster>,
    /// Worker and memory bounds.
    pub pool: PoolLimits,
    /// The charts to compare; `None` compares every screened chart.
    pub selection: Option<BTreeSet<String>>,
    /// Recorded as each new witness's origin.
    pub origin: String,
    /// The pinned Helm runner for live adjudication.
    pub helm: Option<&'static crate::helm::invocation::HelmRunner>,
}

/// One dump-pair comparison over the universe's witnesses and fresh discovery.
pub struct ComparisonRun {
    /// Every witness's outcome.
    pub table: OutcomeTable,
    /// Every witness judged, discovered or replayed, by id.
    pub witnesses: BTreeMap<String, WitnessRecord>,
    /// The witnesses the universe did not hold.
    pub new_witnesses: Vec<WitnessRecord>,
    /// Every artifact's hashes in both dumps.
    pub artifacts: Vec<ArtifactPair>,
    /// Counts for the coverage report.
    pub coverage: Value,
    /// The acceptance core's own report, its witnesses moved out.
    pub comparison: AcceptanceComparison,
}

/// Compares `pair` over fresh discovery and every universe witness of the
/// selected charts, outside discovery caps.
///
/// # Errors
///
/// [`DiffError::Identity`] when a universe witness belongs to other chart
/// inputs or no longer rebuilds; [`DiffError::Infrastructure`] when the
/// comparison, its coverage or a replay fails.
pub fn run_comparison(
    pair: &DumpPair,
    universe: &UniverseState,
    blobs: &BlobStore,
    options: RunOptions,
) -> Result<ComparisonRun, DiffError> {
    let RunOptions {
        kubernetes,
        contexts,
        pool,
        selection,
        origin,
        helm,
    } = options;
    let live = kubernetes.is_some();
    let mut charts = pair.compared_charts(selection.as_ref())?;
    let (identities, expected) =
        attach_replay(&chart_keys(&pair.baseline)?, &mut charts, universe, blobs)?;
    let config = ComparisonConfig {
        kubernetes,
        contexts,
        pool,
        record_witnesses: true,
        helm,
    };
    let mut comparison = compare_charts(charts, &config)
        .map_err(|error| DiffError::Infrastructure(format!("compare: {error:#}")))?;
    comparison.charts_checked = comparison.coverage.len();
    AcceptanceComparison::validate_lint_coverages(&comparison.coverage, live)
        .map_err(|error| DiffError::Infrastructure(format!("lint coverage: {error:#}")))?;
    for chart in &comparison.coverage {
        validate_probe_coverage(chart)
            .map_err(|error| DiffError::Infrastructure(format!("probe coverage: {error:#}")))?;
    }
    if !comparison.screening_failures.is_empty() {
        return Err(DiffError::Infrastructure(format!(
            "screening failures: {}",
            comparison.screening_failures.join("; ")
        )));
    }
    let adjudications = comparison
        .adjudications
        .iter()
        .map(|record| (record.case.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut witnesses = BTreeMap::new();
    let mut outcomes = Vec::new();
    for witness in &comparison.witnesses {
        let identity = identities.get(&witness.chart).ok_or_else(|| {
            DiffError::Infrastructure(format!("witness of unselected chart {}", witness.chart))
        })?;
        let record = witness_record(witness, identity, blobs, &origin)?;
        if witnesses.contains_key(&record.id) {
            continue;
        }
        outcomes.push(classify(
            &record.id,
            witness,
            adjudications.get(&witness.case()).copied(),
        ));
        witnesses.insert(record.id.clone(), record);
    }
    let missing = expected
        .iter()
        .filter(|id| !witnesses.contains_key(*id))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(DiffError::Infrastructure(format!(
            "universe witnesses were not judged: {missing:?}"
        )));
    }
    let new_witnesses = witnesses
        .values()
        .filter(|record| !universe.witnesses.contains_key(&record.id))
        .cloned()
        .collect::<Vec<_>>();
    let table = OutcomeTable::new(outcomes);
    let coverage = json!({
        "charts": comparison.charts_checked,
        "probes_checked": comparison.probes_checked,
        "witnesses": witnesses.len(),
        "replayed": expected.len(),
        "new": new_witnesses.len(),
        "classes": table.class_counts().into_iter().map(|(class, count)| (format!("{class:?}"), count)).collect::<BTreeMap<_, _>>(),
        "flips_screened": comparison.helm_adjudication.screened_flips,
        "flips_adjudicated": comparison.helm_adjudication.flips_adjudicated,
        "adjudication_failures": comparison.helm_adjudication.adjudication_failures.len(),
        "live": live,
    });
    comparison.witnesses.clear();
    Ok(ComparisonRun {
        table,
        witnesses,
        new_witnesses,
        artifacts: pair.artifacts(),
        coverage,
        comparison,
    })
}

/// Attaches each chart's universe witnesses for replay; returns every
/// chart's identity and the ids replayed.
fn attach_replay(
    keys: &BTreeMap<String, capture::ChartKey>,
    charts: &mut [crate::acceptance::ComparedChart],
    universe: &UniverseState,
    blobs: &BlobStore,
) -> Result<(BTreeMap<String, ChartIdentity>, BTreeSet<String>), DiffError> {
    let mut identities = BTreeMap::new();
    let mut expected = BTreeSet::new();
    for chart in charts {
        let key = keys
            .get(&chart.label)
            .ok_or_else(|| DiffError::Identity(format!("no registry key for {}", chart.label)))?;
        let identity = ChartIdentity {
            chart_input_sha256: key.inputs_sha256.clone(),
            recipe_sha256: key.recipe_sha256.clone(),
            defaults: read_coalesced_defaults(key.chart).map_err(|error| {
                DiffError::Infrastructure(format!("{}: {error:#}", chart.label))
            })?,
        };
        for record in universe
            .witnesses
            .values()
            .filter(|record| record.identity.chart == chart.label)
        {
            if record.identity.chart_input_sha256 != identity.chart_input_sha256
                || record.identity.recipe_sha256 != identity.recipe_sha256
            {
                return Err(DiffError::Identity(format!(
                    "witness {} belongs to other inputs of {}; migrate the history",
                    record.id, chart.label
                )));
            }
            chart.replay.push(replay_witness(record, blobs)?);
            expected.insert(record.id.clone());
        }
        identities.insert(chart.label.clone(), identity);
    }
    Ok((identities, expected))
}

/// A reviewed certificate that changed artifact bytes are equivalent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalenceCertificate {
    /// The registry key.
    pub key: String,
    /// SHA-256 of main's bytes it certifies.
    pub baseline_sha256: String,
    /// SHA-256 of the candidate's bytes it certifies.
    pub candidate_sha256: String,
    /// Every JSON pointer that differs.
    pub pointers: Vec<String>,
    /// Who reviewed it.
    pub reviewer: String,
    /// Why the change is equivalent.
    pub reasoning: String,
}

/// One changed artifact and what accounts for its change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactChange {
    /// The artifact's hashes.
    pub artifact: ArtifactPair,
    /// Every JSON pointer whose value differs.
    pub pointers: Vec<String>,
    /// Witnesses of its chart whose outcome changed, as proof links.
    pub proof: Vec<String>,
    /// The certificate that covers it, if any.
    pub certificate: Option<EquivalenceCertificate>,
    /// Whether proof or a certificate accounts for it.
    pub adjudicated: bool,
}

/// Every changed artifact with its structural diff and proof links; a
/// change is adjudicated by a witness with a changed outcome in its chart
/// or by a certificate bound to exactly both hashes and pointers.
///
/// # Errors
///
/// [`DiffError::Identity`] when a changed artifact cannot be read.
pub fn artifact_changes(
    pair: &DumpPair,
    run: &ComparisonRun,
    certificates: &[EquivalenceCertificate],
) -> Result<Vec<ArtifactChange>, DiffError> {
    let keys = chart_keys(&pair.baseline)?;
    let label_of = keys
        .iter()
        .map(|(label, key)| (key.key.clone(), label.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut changes = Vec::new();
    for artifact in &run.artifacts {
        if artifact.baseline_sha256 == artifact.candidate_sha256 {
            continue;
        }
        let parse = |bytes: &[u8]| serde_json::from_slice::<Value>(bytes).ok();
        let pointers = match (
            parse(&pair.baseline.schema(&artifact.key)?.bytes),
            parse(&pair.candidate.schema(&artifact.key)?.bytes),
        ) {
            (Some(left), Some(right)) => json_diff(&left, &right),
            _ => vec![String::new()],
        };
        let proof = label_of
            .get(&artifact.key)
            .map(|label| {
                run.table
                    .outcomes
                    .iter()
                    .filter(|outcome| {
                        outcome.case.starts_with(&format!("{label}: "))
                            && outcome.class != Class::Identical
                    })
                    .map(|outcome| outcome.witness.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let certificate = certificates
            .iter()
            .find(|certificate| {
                certificate.key == artifact.key
                    && certificate.baseline_sha256 == artifact.baseline_sha256
                    && certificate.candidate_sha256 == artifact.candidate_sha256
                    && certificate.pointers == pointers
                    && !certificate.reviewer.is_empty()
                    && !certificate.reasoning.is_empty()
            })
            .cloned();
        changes.push(ArtifactChange {
            artifact: artifact.clone(),
            adjudicated: !proof.is_empty() || certificate.is_some(),
            pointers,
            proof,
            certificate,
        });
    }
    Ok(changes)
}

/// Every JSON pointer at which `left` and `right` differ, at the shallowest
/// differing member.
#[must_use]
pub fn json_diff(left: &Value, right: &Value) -> Vec<String> {
    fn walk(left: &Value, right: &Value, pointer: &str, out: &mut Vec<String>) {
        match (left, right) {
            (Value::Object(left), Value::Object(right)) => {
                let keys = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
                for key in keys {
                    let child = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                    match (left.get(key), right.get(key)) {
                        (Some(left), Some(right)) => walk(left, right, &child, out),
                        _ => out.push(child),
                    }
                }
            }
            (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
                for (index, (left, right)) in left.iter().zip(right).enumerate() {
                    walk(left, right, &format!("{pointer}/{index}"), out);
                }
            }
            _ => {
                if canonical_json(left) != canonical_json(right) {
                    out.push(pointer.to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(left, right, "", &mut out);
    out
}
