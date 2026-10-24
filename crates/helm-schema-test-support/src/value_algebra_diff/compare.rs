//! Witness outcomes, their comparison classes and the construction policy.
//!
//! [`classify`] turns one witness's baseline and candidate schema verdicts,
//! and the pinned oracle's answer when there is one, into a
//! [`Class`]. The construction policy then works on sets over the whole
//! universe: the regression set may only shrink after the reviewed seed,
//! earned closures may never be lost, and a ledger must name the regression
//! set exactly. Classes are never counted against an allowance.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use test_util::helm_values::AcceptanceDocument;

use super::DiffError;
use super::history::{
    BlobStore, DocumentRecord, FORMAT_VERSION, InstanceRecord, SCHEMA_ACCEPTANCE, WitnessIdentity,
    WitnessMode, WitnessRecord, canonical_json, canonical_sha256,
};
use crate::acceptance::discovery::ProbeInstance;
use crate::acceptance::screen::{ScreenedDocument, ScreenedProbe};
use crate::acceptance::{
    AdjudicationFailure, AdjudicationRecord, FailureKind, HelmFlipVerdict, ScreenedWitness,
};
use crate::source_digest::sha256_hex;

/// The comparison class of one witness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum Class {
    /// The same verdict; byte equality is reported separately.
    Identical,
    /// Main and candidate wrong alike under the pinned oracle.
    Inherited,
    /// A pre-approved declared-types policy identity; no closure credit.
    Policy,
    /// Main wrong, candidate correct, with complete live evidence.
    Closure,
    /// Main right, candidate wrong, or a different failure.
    Regression,
    /// A reviewed equivalence of changed bytes.
    EquivalentChange,
    /// A changed verdict without sufficient evidence.
    Unadjudicated,
    /// A provenance, coverage, input, tool or execution failure.
    Infrastructure,
}

/// What the pinned oracle says about a witness's document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnswer {
    /// Helm and Kubernetes accept it.
    Accepts,
    /// Helm aborts, lint rejects or Kubernetes rejects it.
    Rejects,
    /// The rejection is exactly the declared-types authoring policy.
    Policy,
    /// The evidence cannot decide.
    Undecided,
    /// Not adjudicated: no transition, or the schemas alone judged.
    NotAsked,
}

/// One witness's outcome in one comparison.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessOutcome {
    /// The witness id.
    pub witness: String,
    /// `chart: name`.
    pub case: String,
    /// Whether main's schema accepts it.
    pub baseline_accepts: bool,
    /// Whether the candidate's schema accepts it.
    pub candidate_accepts: bool,
    /// Whether the verdicts differ or the candidate rejects for a new reason.
    pub transition: bool,
    /// The oracle's answer.
    pub oracle: OracleAnswer,
    /// The class.
    pub class: Class,
    /// The error direction of a regression or closure.
    pub direction: Option<String>,
    /// The adjudication's evidence directory or failure message.
    pub detail: Option<String>,
}

/// A regression's identity: its witness and its error direction.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegressionKey {
    /// The witness id.
    pub witness: String,
    /// The error direction.
    pub direction: String,
}

/// The oracle answer `verdict` implies, with the candidate verdict the
/// adjudicated document had when it implies one.
fn verdict_answer(verdict: &HelmFlipVerdict) -> (OracleAnswer, Option<bool>) {
    match verdict {
        HelmFlipVerdict::Collapsed => (OracleAnswer::NotAsked, None),
        HelmFlipVerdict::TighteningMatchedLintRejection
        | HelmFlipVerdict::TighteningMatchedHelmAbort
        | HelmFlipVerdict::TighteningMatchedKubernetesRejection => {
            (OracleAnswer::Rejects, Some(false))
        }
        HelmFlipVerdict::CandidateAcceptsLintRejects
        | HelmFlipVerdict::CandidateAcceptsHelmAborts
        | HelmFlipVerdict::CandidateAcceptsKubernetesRejects
        | HelmFlipVerdict::UninformativeBaselineFalseAcceptance(_) => {
            (OracleAnswer::Rejects, Some(true))
        }
        HelmFlipVerdict::LooseningMatchedLintValidation
        | HelmFlipVerdict::LooseningMatchedKubernetesValidation
        | HelmFlipVerdict::LooseningMatchedDefaultsViolations => {
            (OracleAnswer::Accepts, Some(true))
        }
        HelmFlipVerdict::LooseningWithUncertainKubernetes(_) => {
            (OracleAnswer::Undecided, Some(true))
        }
        HelmFlipVerdict::TighteningMatchedDeclaredTypesPolicy => {
            (OracleAnswer::Policy, Some(false))
        }
    }
}

/// Classifies `witness` given its adjudication, if any.
#[must_use]
pub fn classify(
    witness_id: &str,
    witness: &ScreenedWitness,
    adjudication: Option<&AdjudicationRecord>,
) -> WitnessOutcome {
    let (baseline, candidate) = (witness.baseline_accepts, witness.candidate_accepts);
    let unchanged = baseline == candidate && !witness.transition;
    let (oracle, implied, detail, infrastructure) = match adjudication.map(|record| &record.outcome)
    {
        None => (OracleAnswer::NotAsked, None, None, false),
        Some(Ok((verdict, evidence))) => {
            let (oracle, implied) = verdict_answer(verdict);
            (oracle, implied, Some(evidence.display().to_string()), false)
        }
        Some(Err(AdjudicationFailure { kind, message })) => match kind {
            FailureKind::UnattributedRejection => (
                OracleAnswer::Accepts,
                Some(false),
                Some(message.clone()),
                false,
            ),
            FailureKind::Undecided => (OracleAnswer::Undecided, None, Some(message.clone()), false),
            FailureKind::Infrastructure => {
                (OracleAnswer::Undecided, None, Some(message.clone()), true)
            }
        },
    };
    let mut direction = None;
    let class = if infrastructure {
        Class::Infrastructure
    } else if implied.is_some_and(|implied| implied != candidate) {
        // Helm's coalesced document and the screened composition disagree.
        Class::Unadjudicated
    } else {
        match oracle {
            OracleAnswer::NotAsked if adjudication.is_some() || unchanged => Class::Identical,
            OracleAnswer::NotAsked | OracleAnswer::Undecided => {
                if unchanged {
                    Class::Identical
                } else {
                    Class::Unadjudicated
                }
            }
            OracleAnswer::Policy => Class::Policy,
            OracleAnswer::Accepts | OracleAnswer::Rejects => {
                let truth = oracle == OracleAnswer::Accepts;
                match (baseline == truth, candidate == truth) {
                    (true, true) => Class::Identical,
                    (false, false) if unchanged => Class::Inherited,
                    (false, false) => {
                        direction = Some("changed-failure".to_string());
                        Class::Regression
                    }
                    (true, false) => {
                        direction = Some(
                            if candidate {
                                "accepts-oracle-rejection"
                            } else {
                                "rejects-oracle-acceptance"
                            }
                            .to_string(),
                        );
                        Class::Regression
                    }
                    (false, true) => {
                        direction = Some(
                            if candidate {
                                "accepts-oracle-acceptance"
                            } else {
                                "rejects-oracle-rejection"
                            }
                            .to_string(),
                        );
                        Class::Closure
                    }
                }
            }
        }
    };
    WitnessOutcome {
        witness: witness_id.to_string(),
        case: witness.case(),
        baseline_accepts: baseline,
        candidate_accepts: candidate,
        transition: witness.transition,
        oracle,
        class,
        direction,
        detail,
    }
}

/// Everything one comparison concluded, by witness.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeTable {
    /// Every witness's outcome, sorted by witness id.
    pub outcomes: Vec<WitnessOutcome>,
}

impl OutcomeTable {
    /// The table of `outcomes`, sorted.
    #[must_use]
    pub fn new(mut outcomes: Vec<WitnessOutcome>) -> Self {
        outcomes.sort_by(|left, right| left.witness.cmp(&right.witness));
        Self { outcomes }
    }

    /// The regression set.
    #[must_use]
    pub fn regressions(&self) -> BTreeSet<RegressionKey> {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.class == Class::Regression)
            .map(|outcome| RegressionKey {
                witness: outcome.witness.clone(),
                direction: outcome.direction.clone().unwrap_or_default(),
            })
            .collect()
    }

    /// The earned closure set.
    #[must_use]
    pub fn closures(&self) -> BTreeSet<String> {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.class == Class::Closure)
            .map(|outcome| outcome.witness.clone())
            .collect()
    }

    /// Outcomes per class.
    #[must_use]
    pub fn class_counts(&self) -> BTreeMap<Class, usize> {
        let mut counts = BTreeMap::new();
        for outcome in &self.outcomes {
            *counts.entry(outcome.class).or_insert(0) += 1;
        }
        counts
    }
}

/// One reviewed regression ledger row.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LedgerRow {
    /// The witness id.
    pub witness: String,
    /// Its error direction.
    pub direction: String,
    /// Who owns its repair.
    pub owner: String,
    /// Why it regressed.
    pub cause: String,
    /// The checkpoint ordinal planned to repair it.
    pub repair_checkpoint: u32,
}

/// A reviewed seed: the exact regression set C02 may establish.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedReview {
    /// [`FORMAT_VERSION`].
    pub format_version: u32,
    /// The reviewer, who must be the plan's.
    pub reviewer: String,
    /// The reviewed rows.
    pub rows: Vec<LedgerRow>,
}

/// The sets a predecessor receipt recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorSets {
    /// Its construction ordinal.
    pub ordinal: u32,
    /// Its regression set.
    pub regressions: BTreeSet<RegressionKey>,
    /// Its earned closures.
    pub closures: BTreeSet<String>,
}

/// Why a construction comparison fails, empty when it passes.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PolicyVerdict {
    /// Semantic, unadjudicated or monotonicity problems: exit code 1.
    pub semantic: Vec<String>,
    /// Infrastructure outcomes: exit code 3.
    pub infrastructure: Vec<String>,
}

/// What the construction policy is applied to.
pub struct ConstructionRequest<'a> {
    /// The checkpoint's ordinal.
    pub ordinal: u32,
    /// Whether it must make semantic progress.
    pub semantic: bool,
    /// Its outcomes.
    pub table: &'a OutcomeTable,
    /// The predecessor's sets; genesis has empty ones at ordinal 0.
    pub prior: &'a PriorSets,
    /// The regression ledger, which must name the regression set exactly.
    pub ledger: &'a [LedgerRow],
    /// The seed review, allowed only at the seed ordinal.
    pub seed: Option<&'a SeedReview>,
    /// The plan's seed ordinal.
    pub seed_ordinal: u32,
    /// The plan's reviewer.
    pub reviewer: &'a str,
}

/// Applies the construction policy.
///
/// # Errors
///
/// [`DiffError::Request`] when a seed review is supplied outside the seed
/// ordinal or missing at it; [`DiffError::Identity`] when the ordinal does
/// not follow the predecessor's.
pub fn construction_policy(request: &ConstructionRequest<'_>) -> Result<PolicyVerdict, DiffError> {
    let ConstructionRequest {
        ordinal,
        semantic,
        table,
        prior,
        ledger,
        seed,
        seed_ordinal,
        reviewer,
    } = request;
    if *ordinal != prior.ordinal + 1 {
        return Err(DiffError::Identity(format!(
            "checkpoint {ordinal} does not follow its predecessor's ordinal {}",
            prior.ordinal
        )));
    }
    let seeding = ordinal == seed_ordinal;
    if seeding != seed.is_some() {
        return Err(DiffError::Request(if seeding {
            format!("checkpoint {ordinal} establishes the seed and needs --seed-review")
        } else {
            format!("only checkpoint {seed_ordinal} may supply --seed-review")
        }));
    }
    let mut verdict = PolicyVerdict::default();
    for outcome in &table.outcomes {
        match outcome.class {
            Class::Infrastructure => verdict.infrastructure.push(format!(
                "{}: {}",
                outcome.case,
                outcome
                    .detail
                    .as_deref()
                    .unwrap_or("infrastructure failure")
            )),
            Class::Unadjudicated => verdict
                .semantic
                .push(format!("UNADJUDICATED {}", outcome.case)),
            Class::Identical
            | Class::Inherited
            | Class::Policy
            | Class::Closure
            | Class::Regression
            | Class::EquivalentChange => {}
        }
    }
    let regressions = table.regressions();
    let closures = table.closures();
    if *ordinal == 1 {
        for outcome in &table.outcomes {
            if !matches!(outcome.class, Class::Identical | Class::Inherited) {
                verdict.semantic.push(format!(
                    "checkpoint 1 must equal main, but {} is {:?}",
                    outcome.case, outcome.class
                ));
            }
        }
    }
    if let Some(seed) = seed {
        verdict
            .semantic
            .extend(seed_problems(seed, reviewer, &regressions));
    } else if *ordinal != 1 {
        for added in regressions.difference(&prior.regressions) {
            verdict.semantic.push(format!(
                "new regression {} ({})",
                added.witness, added.direction
            ));
        }
    }
    for lost in prior.closures.difference(&closures) {
        verdict.semantic.push(format!("lost closure {lost}"));
    }
    if *semantic
        && *ordinal > *seed_ordinal
        && regressions == prior.regressions
        && closures == prior.closures
    {
        verdict.semantic.push(format!(
            "semantic checkpoint {ordinal} neither shrinks the regression set nor earns a closure"
        ));
    }
    verdict
        .semantic
        .extend(ledger_problems(ledger, &regressions));
    Ok(verdict)
}

/// Why `seed` is not exactly the measured `regressions`, reviewed by `reviewer`.
fn seed_problems(
    seed: &SeedReview,
    reviewer: &str,
    regressions: &BTreeSet<RegressionKey>,
) -> Vec<String> {
    let mut problems = Vec::new();
    if seed.format_version != FORMAT_VERSION || seed.reviewer != reviewer {
        problems.push("the seed review is not a version-1 review by the plan's reviewer".into());
    }
    let seeded = keys(&seed.rows);
    for missing in regressions.difference(&seeded) {
        problems.push(format!(
            "unreviewed seed regression {} ({})",
            missing.witness, missing.direction
        ));
    }
    for stale in seeded.difference(regressions) {
        problems.push(format!(
            "reviewed seed row {} ({}) is not a measured regression",
            stale.witness, stale.direction
        ));
    }
    problems
}

/// Why `ledger` does not name `regressions` exactly.
fn ledger_problems(ledger: &[LedgerRow], regressions: &BTreeSet<RegressionKey>) -> Vec<String> {
    let mut problems = Vec::new();
    let ledgered = keys(ledger);
    if ledgered.len() != ledger.len() {
        problems.push("the ledger lists a row twice".into());
    }
    for stale in ledgered.difference(regressions) {
        problems.push(format!(
            "stale ledger row {} ({})",
            stale.witness, stale.direction
        ));
    }
    for added in regressions.difference(&ledgered) {
        problems.push(format!(
            "regression {} ({}) is not in the ledger",
            added.witness, added.direction
        ));
    }
    problems
}

fn keys(rows: &[LedgerRow]) -> BTreeSet<RegressionKey> {
    rows.iter()
        .map(|row| RegressionKey {
            witness: row.witness.clone(),
            direction: row.direction.clone(),
        })
        .collect()
}

/// What identifies a chart's experiments besides each witness's values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartIdentity {
    /// The producer's input digest of the chart's recipe.
    pub chart_input_sha256: String,
    /// SHA-256 of the canonical recipe description.
    pub recipe_sha256: String,
    /// The chart's coalesced defaults, which template probes compose over.
    pub defaults: Value,
}

/// The persistent record of `witness`, its bytes stored in `blobs`.
///
/// # Errors
///
/// [`DiffError::Identity`] when the witness names neither or both lanes;
/// [`DiffError::Infrastructure`] when a blob cannot be written.
pub fn witness_record(
    witness: &ScreenedWitness,
    chart: &ChartIdentity,
    blobs: &BlobStore,
    origin: &str,
) -> Result<WitnessRecord, DiffError> {
    let serialize = |value: &Value| {
        serde_json::to_vec_pretty(value)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))
    };
    let (mode, values_file, contexts, instance, document, defaults_blob) =
        match (&witness.instance, &witness.probe.document) {
            (Some(instance), None) => (
                WitnessMode::TemplateProbe,
                serialize(&witness.probe.overlay)?,
                vec![json!({})],
                Some(match instance {
                    ProbeInstance::Defaults => InstanceRecord::Defaults,
                    ProbeInstance::SparseOverride(value) => {
                        InstanceRecord::SparseOverride(value.clone())
                    }
                    ProbeInstance::Coalesced(value) => InstanceRecord::Coalesced(value.clone()),
                }),
                None,
                Some(blobs.put(&canonical_json(&chart.defaults))?),
            ),
            (None, Some(document)) => (
                match document.kind {
                    AcceptanceDocument::LintRaw => WitnessMode::LintRaw,
                    AcceptanceDocument::LintCoalescedTwice => WitnessMode::LintCoalescedTwice,
                    AcceptanceDocument::Template => WitnessMode::Template,
                },
                document.values_file.clone(),
                document.control_overlays.clone(),
                None,
                Some(DocumentRecord {
                    lint_control: document.lint_control,
                    control_overlays: document.control_overlays.clone(),
                    composed_blob: blobs.put(&canonical_json(&document.values))?,
                }),
                None,
            ),
            (Some(_), Some(_)) | (None, None) => {
                return Err(DiffError::Identity(format!(
                    "witness {} names neither or both lanes",
                    witness.case()
                )));
            }
        };
    let identity = WitnessIdentity {
        chart: witness.chart.clone(),
        chart_input_sha256: chart.chart_input_sha256.clone(),
        recipe_sha256: chart.recipe_sha256.clone(),
        values_sha256: sha256_hex(&values_file),
        mode,
        kubernetes_version: witness.kubernetes_version.clone(),
        context_sha256: sha256_hex(&canonical_json(&Value::Array(contexts))),
        observation: SCHEMA_ACCEPTANCE.to_string(),
    };
    let mut origins = witness
        .probe
        .document
        .as_ref()
        .map_or_else(Vec::new, |document| document.origins.clone());
    origins.push(origin.to_string());
    Ok(WitnessRecord {
        format_version: FORMAT_VERSION,
        id: identity.id()?,
        identity,
        name: witness.probe.name.clone(),
        overlay: witness.probe.overlay.clone(),
        values_blob: blobs.put(&values_file)?,
        composed_sha256: sha256_hex(&canonical_json(&witness.composed)),
        defaults_blob,
        instance,
        document,
        origins,
    })
}

/// `record` as a witness to replay, its documents rebuilt from its blobs
/// and checked against its recorded hash.
///
/// # Errors
///
/// [`DiffError::Identity`] when a blob is missing or the rebuilt document
/// differs from the recorded one.
pub fn replay_witness(
    record: &WitnessRecord,
    blobs: &BlobStore,
) -> Result<ScreenedWitness, DiffError> {
    let parse = |bytes: &[u8]| {
        serde_json::from_slice::<Value>(bytes)
            .map_err(|error| DiffError::Identity(format!("witness {}: {error}", record.id)))
    };
    let (instance, document, composed) =
        match (&record.instance, &record.document, &record.defaults_blob) {
            (Some(instance), None, Some(defaults)) => {
                let defaults = parse(&blobs.get(defaults)?)?;
                let (instance, composed) = match instance {
                    InstanceRecord::Defaults => (ProbeInstance::Defaults, defaults),
                    InstanceRecord::SparseOverride(value) => (
                        ProbeInstance::SparseOverride(value.clone()),
                        test_util::helm_values::coalesce_tables(value, &defaults),
                    ),
                    InstanceRecord::Coalesced(value) => {
                        (ProbeInstance::Coalesced(value.clone()), value.clone())
                    }
                };
                (Some(instance), None, composed)
            }
            (None, Some(document), None) => {
                let composed = parse(&blobs.get(&document.composed_blob)?)?;
                let kind = match record.identity.mode {
                    WitnessMode::LintRaw => AcceptanceDocument::LintRaw,
                    WitnessMode::LintCoalescedTwice => AcceptanceDocument::LintCoalescedTwice,
                    WitnessMode::Template => AcceptanceDocument::Template,
                    WitnessMode::TemplateProbe => {
                        return Err(DiffError::Identity(format!(
                            "witness {} is a document of the template-probe lane",
                            record.id
                        )));
                    }
                };
                let document = ScreenedDocument {
                    kind,
                    lint_control: document.lint_control,
                    values: composed.clone(),
                    defaults: None,
                    kubernetes_version: record.identity.kubernetes_version.clone(),
                    origins: record
                        .origins
                        .iter()
                        .filter(|origin| !origin.starts_with("attempt:"))
                        .cloned()
                        .collect(),
                    values_file: blobs.get(&record.values_blob)?,
                    control_overlays: document.control_overlays.clone(),
                };
                (None, Some(document), composed)
            }
            _ => {
                return Err(DiffError::Identity(format!(
                    "witness {} names neither or both lanes",
                    record.id
                )));
            }
        };
    if sha256_hex(&canonical_json(&composed)) != record.composed_sha256 {
        return Err(DiffError::Identity(format!(
            "witness {} rebuilds to another document",
            record.id
        )));
    }
    Ok(ScreenedWitness {
        chart: record.identity.chart.clone(),
        probe: ScreenedProbe {
            name: record.name.clone(),
            overlay: record.overlay.clone(),
            document,
        },
        instance,
        composed,
        kubernetes_version: record.identity.kubernetes_version.clone(),
        baseline_accepts: false,
        candidate_accepts: false,
        screened: false,
        transition: false,
    })
}

/// SHA-256 of the canonical serialization of `value`, for receipts.
///
/// # Errors
///
/// [`DiffError::Infrastructure`] when it cannot be serialized.
pub fn table_hash<T: Serialize>(value: &T) -> Result<String, DiffError> {
    canonical_sha256(value)
}
