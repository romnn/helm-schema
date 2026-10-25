//! The value-algebra differential gate: two captured revisions compared
//! over a persistent, append-only witness universe under an explicit
//! construction policy, with hash-chained receipts.
//!
//! [`revision`] captures a revision and binds everything it measured;
//! [`capture`] verifies a producer dump against its own manifest. The
//! comparison itself is [`crate::acceptance`]'s policy-free
//! core; this module adds witness identity, history and policy.

pub mod admission;
pub mod build;
pub mod capture;
pub mod commands;
pub mod compare;
pub mod controls;
pub mod history;
pub mod revision;

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
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::acceptance::discovery::{read_coalesced_defaults, validate_probe_coverage};
use crate::acceptance::pool::PoolLimits;
use crate::acceptance::screen::ImportedRoster;
use crate::acceptance::{
    AcceptanceComparison, ComparisonConfig, Replay, ScreenedWitness, compare_charts,
};
use crate::helm::adjudication::OfflineKubernetesValidator;
use capture::{ArtifactPair, DumpPair, chart_keys};
use compare::{
    ChartIdentity, Class, JudgedWitness, OutcomeTable, classify, replay_witness, witness_record,
};
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
    /// How many distinct witnesses were judged, discovered or replayed.
    pub witness_count: usize,
    /// The witnesses the universe did not hold.
    pub new_witnesses: Vec<WitnessRecord>,
    /// Every artifact's hashes in both dumps.
    pub artifacts: Vec<ArtifactPair>,
    /// Counts for the coverage report.
    pub coverage: Value,
    /// The acceptance core's own report, its witnesses moved out.
    pub comparison: AcceptanceComparison,
    /// Coverage, screening and replay failures found after discovery: they
    /// fail the comparison, but only after its discoveries are journaled.
    pub failures: Vec<String>,
}

/// Compares `pair` over fresh discovery and every universe witness of the
/// selected charts, outside discovery caps. Coverage and screening failures
/// do not discard what was discovered: they are returned in
/// [`ComparisonRun::failures`] beside every witness record.
///
/// # Errors
///
/// [`DiffError::Identity`] when a universe witness belongs to other chart
/// inputs or no longer rebuilds; [`DiffError::Infrastructure`] when the
/// comparison cannot run at all.
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
    let recorder = Arc::new(Recorder {
        identities,
        known: universe.witnesses.keys().cloned().collect(),
        blobs: blobs.clone(),
        origin,
        judged: Mutex::new(Judged::default()),
    });
    let sink = recorder.clone();
    let config = ComparisonConfig {
        kubernetes,
        contexts,
        pool,
        witnesses: Some(Arc::new(move |witness| sink.record(&witness))),
        helm,
    };
    let mut comparison = compare_charts(charts, &config)
        .map_err(|error| DiffError::Infrastructure(format!("compare: {error:#}")))?;
    drop(config);
    let Judged {
        witnesses,
        new,
        failures: mut recorded_failures,
    } = Arc::into_inner(recorder)
        .and_then(|recorder| recorder.judged.into_inner().ok())
        .ok_or_else(|| DiffError::Infrastructure("the witness recorder is still shared".into()))?;
    comparison.charts_checked = comparison.coverage.len();
    let mut failures = Vec::new();
    if let Err(error) = AcceptanceComparison::validate_lint_coverages(&comparison.coverage, live) {
        failures.push(format!("lint coverage: {error:#}"));
    }
    for chart in &comparison.coverage {
        if let Err(error) = validate_probe_coverage(chart) {
            failures.push(format!("probe coverage: {error:#}"));
        }
    }
    failures.extend(
        comparison
            .screening_failures
            .iter()
            .map(|failure| format!("screening failure: {failure}")),
    );
    failures.append(&mut recorded_failures);
    let adjudications = comparison
        .adjudications
        .iter()
        .map(|record| (record.case.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let outcomes = witnesses
        .values()
        .map(|witness| classify(witness, adjudications.get(&witness.case).copied()))
        .collect();
    let missing = expected
        .iter()
        .filter(|id| !witnesses.contains_key(*id))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        failures.push(format!("universe witnesses were not judged: {missing:?}"));
    }
    let new_witnesses = new.into_values().collect::<Vec<_>>();
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
    Ok(ComparisonRun {
        table,
        witness_count: witnesses.len(),
        new_witnesses,
        artifacts: pair.artifacts(),
        coverage,
        comparison,
        failures,
    })
}

/// What the recorder keeps of every judged witness: its verdicts, and the
/// record of each witness the universe does not hold.
#[derive(Default)]
struct Judged {
    witnesses: BTreeMap<String, JudgedWitness>,
    new: BTreeMap<String, WitnessRecord>,
    failures: Vec<String>,
}

/// Turns each witness the screening workers judge into its record, storing
/// its documents as blobs, and keeps only its verdicts.
struct Recorder {
    identities: BTreeMap<String, ChartIdentity>,
    known: BTreeSet<String>,
    blobs: BlobStore,
    origin: String,
    judged: Mutex<Judged>,
}

impl Recorder {
    fn record(&self, witness: &ScreenedWitness) -> color_eyre::Result<()> {
        let record = match self.identities.get(&witness.chart) {
            Some(identity) => Some(witness_record(
                witness,
                identity,
                &self.blobs,
                &self.origin,
            )?),
            None => None,
        };
        let mut judged = self
            .judged
            .lock()
            .map_err(|_| color_eyre::eyre::eyre!("a witness recorder panicked"))?;
        let Some(record) = record else {
            judged
                .failures
                .push(format!("witness of unselected chart {}", witness.chart));
            return Ok(());
        };
        if judged.witnesses.contains_key(&record.id) {
            return Ok(());
        }
        judged.witnesses.insert(
            record.id.clone(),
            JudgedWitness::of(record.id.clone(), witness),
        );
        if !self.known.contains(&record.id) {
            judged.new.insert(record.id.clone(), record);
        }
        Ok(())
    }
}

/// Attaches each chart's universe witnesses for replay, materialized one
/// at a time as the chart's worker judges them; returns every chart's
/// identity and the ids replayed.
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
        let mut records = Vec::new();
        for record in universe.witnesses.values().filter(|record| {
            record.identity.chart == chart.label
                && record.identity.mode != history::WitnessMode::Observation
        }) {
            if record.identity.chart_input_sha256 != identity.chart_input_sha256
                || record.identity.recipe_sha256 != identity.recipe_sha256
            {
                return Err(DiffError::Identity(format!(
                    "witness {} belongs to other inputs of {}; migrate the history",
                    record.id, chart.label
                )));
            }
            expected.insert(record.id.clone());
            records.push(record.clone());
        }
        let blobs = blobs.clone();
        chart.replay = Replay::new(records.into_iter().map(move |record| {
            replay_witness(&record, &blobs).map_err(|error| color_eyre::eyre::eyre!("{error}"))
        }));
        identities.insert(chart.label.clone(), identity);
    }
    Ok((identities, expected))
}

/// A reviewed certificate for changed artifact bytes, bound to both hashes
/// and every differing pointer; it may name the witnesses that prove the
/// change, each of which must have a changed outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquivalenceCertificate {
    /// The artifact key.
    pub key: String,
    /// SHA-256 of main's bytes it certifies.
    pub baseline_sha256: String,
    /// SHA-256 of the candidate's bytes it certifies.
    pub candidate_sha256: String,
    /// Every JSON pointer that differs.
    pub pointers: Vec<String>,
    /// Witnesses whose changed outcomes prove the change, if any.
    #[serde(default)]
    pub witnesses: Vec<String>,
    /// Who reviewed it.
    pub reviewer: String,
    /// Why the change is accounted for.
    pub reasoning: String,
}

/// Reads both sides of a changed artifact.
pub type ArtifactBytes<'a> = dyn Fn(&ArtifactPair) -> Result<(Vec<u8>, Vec<u8>), DiffError> + 'a;

/// One changed artifact and what accounts for its change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactChange {
    /// The artifact's hashes.
    pub artifact: ArtifactPair,
    /// Every JSON pointer whose value differs.
    pub pointers: Vec<String>,
    /// The certificate that covers it, if any.
    pub certificate: Option<EquivalenceCertificate>,
    /// Why the certificate, if any, does not hold.
    pub problems: Vec<String>,
    /// Whether a holding certificate accounts for it.
    pub adjudicated: bool,
}

/// Every changed artifact with its structural diff. A change is accounted
/// for only by a certificate bound to exactly both hashes and its pointers,
/// reviewed by `reviewer` when one is required, whose named witnesses all
/// have changed outcomes; an unrelated witness of the chart never is.
///
/// # Errors
///
/// [`DiffError::Identity`] when a changed artifact cannot be read.
pub fn artifact_changes(
    artifacts: &[ArtifactPair],
    bytes: &ArtifactBytes<'_>,
    table: &OutcomeTable,
    certificates: &[EquivalenceCertificate],
    reviewer: Option<&str>,
) -> Result<Vec<ArtifactChange>, DiffError> {
    let changed_outcomes = table
        .entries()
        .into_iter()
        .filter(|entry| entry.class != Class::Identical)
        .map(|entry| entry.witness.to_string())
        .collect::<BTreeSet<_>>();
    let mut changes = Vec::new();
    for artifact in artifacts {
        if artifact.baseline_sha256 == artifact.candidate_sha256 {
            continue;
        }
        let (left, right) = bytes(artifact)?;
        let parse = |bytes: &[u8]| serde_json::from_slice::<Value>(bytes).ok();
        let pointers = match (parse(&left), parse(&right)) {
            (Some(left), Some(right)) => json_diff(&left, &right),
            _ => vec![String::new()],
        };
        let certificate = certificates
            .iter()
            .find(|certificate| certificate.key == artifact.key)
            .cloned();
        let mut problems = Vec::new();
        match &certificate {
            None => problems.push("no certificate".to_string()),
            Some(certificate) => {
                if certificate.baseline_sha256 != artifact.baseline_sha256
                    || certificate.candidate_sha256 != artifact.candidate_sha256
                {
                    problems.push("the certificate names other bytes".into());
                }
                if certificate.pointers != pointers {
                    problems.push("the certificate names other pointers".into());
                }
                if certificate.reasoning.trim().is_empty()
                    || certificate.reviewer.trim().is_empty()
                    || reviewer.is_some_and(|reviewer| certificate.reviewer != reviewer)
                {
                    problems.push("the certificate is not reviewed".into());
                }
                for witness in &certificate.witnesses {
                    if !changed_outcomes.contains(witness) {
                        problems.push(format!("proof witness {witness} has no changed outcome"));
                    }
                }
            }
        }
        changes.push(ArtifactChange {
            artifact: artifact.clone(),
            adjudicated: problems.is_empty(),
            pointers,
            certificate,
            problems,
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
