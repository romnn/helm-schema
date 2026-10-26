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
use super::admission::AdmissionOutcome;
use super::controls::ObservationOutcome;
use super::history::{
    BlobStore, DocumentRecord, FORMAT_VERSION, InstanceRecord, SCHEMA_ACCEPTANCE, WitnessIdentity,
    WitnessMode, WitnessRecord, canonical_json,
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
    /// The adjudication's failure message, if it failed.
    pub detail: Option<String>,
    /// The adjudication's evidence directory, retained under the payload.
    pub evidence: Option<String>,
    /// SHA-256 of the candidate's semantic answer: its verdict, the oracle's
    /// answer and verdict, and the assertions it newly fails.
    pub answer_sha256: String,
}

/// A regression's identity: its witness, its error direction and the
/// candidate's exact answer, so a changed wrong answer is a new regression.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegressionKey {
    /// The witness id.
    pub witness: String,
    /// The error direction.
    pub direction: String,
    /// SHA-256 of the candidate's semantic answer.
    pub answer_sha256: String,
}

/// One outcome of any table, as the policies see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<'a> {
    /// The witness id.
    pub witness: &'a str,
    /// The human-readable case.
    pub case: &'a str,
    /// The class.
    pub class: Class,
    /// The error direction of a regression or closure.
    pub direction: Option<&'a str>,
    /// SHA-256 of the candidate's semantic answer.
    pub answer_sha256: &'a str,
    /// Evidence or failure detail.
    pub detail: Option<&'a str>,
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

/// What classifying a judged witness needs: its verdicts, never its
/// documents, so a comparison holds one of these per witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JudgedWitness {
    /// The witness id.
    pub id: String,
    /// `chart: name`, for reports only.
    pub case: String,
    /// [`crate::acceptance::probe_key`] of its probe: what joins it to its
    /// adjudication.
    pub key: String,
    /// Whether screening proposed it to the oracle.
    pub screened: bool,
    /// Whether the baseline accepts it.
    pub baseline_accepts: bool,
    /// Whether the candidate accepts it.
    pub candidate_accepts: bool,
    /// Whether the verdicts differ or the candidate rejects for a new reason.
    pub transition: bool,
    /// The candidate's violations for a transition it rejects.
    pub candidate_violations: Vec<String>,
}

impl JudgedWitness {
    /// The verdicts of `witness`, identified as `id`.
    #[must_use]
    pub fn of(id: String, witness: &ScreenedWitness) -> Self {
        Self {
            id,
            case: witness.case(),
            key: crate::acceptance::probe_key(&witness.chart, &witness.probe),
            screened: witness.screened,
            baseline_accepts: witness.baseline_accepts,
            candidate_accepts: witness.candidate_accepts,
            transition: witness.transition,
            candidate_violations: witness.candidate_violations.clone(),
        }
    }
}

/// What one adjudication says.
struct Reading {
    oracle: OracleAnswer,
    /// The candidate verdict the oracle's answer implies, if any.
    implied: Option<bool>,
    detail: Option<String>,
    infrastructure: bool,
    severity: String,
}

fn reading(record: Option<&AdjudicationRecord>) -> Reading {
    let (oracle, implied, detail, infrastructure, severity) = match record
        .map(|record| &record.outcome)
    {
        None => (OracleAnswer::NotAsked, None, None, false, String::new()),
        Some(Ok((verdict, _))) => {
            let (oracle, implied) = verdict_answer(verdict);
            (oracle, implied, None, false, format!("{verdict:?}"))
        }
        Some(Err(AdjudicationFailure { kind, message })) => {
            let severity = format!("{kind:?}");
            let detail = Some(message.clone());
            match kind {
                FailureKind::UnattributedRejection => {
                    (OracleAnswer::Accepts, Some(false), detail, false, severity)
                }
                FailureKind::Undecided => (OracleAnswer::Undecided, None, detail, false, severity),
                FailureKind::Infrastructure => {
                    (OracleAnswer::Undecided, None, detail, true, severity)
                }
            }
        }
    };
    Reading {
        oracle,
        implied,
        detail,
        infrastructure,
        severity,
    }
}

/// Classifies `witness` given what the oracle said. A proposed witness the
/// live oracle never answered is UNADJUDICATED, never IDENTICAL.
#[must_use]
pub fn classify(witness: &JudgedWitness, record: Option<&AdjudicationRecord>) -> WitnessOutcome {
    let witness_id = witness.id.as_str();
    let (baseline, candidate) = (witness.baseline_accepts, witness.candidate_accepts);
    let unchanged = baseline == candidate && !witness.transition;
    let missing = witness.screened && record.is_none();
    let evidence = record
        .and_then(|record| record.evidence.as_ref())
        .map(|path| path.display().to_string());
    let Reading {
        oracle,
        implied,
        detail,
        infrastructure,
        severity,
    } = reading(record);

    let answer_sha256 = sha256_hex(&canonical_json(&json!({
        "candidate_accepts": candidate,
        "oracle": oracle,
        "verdict": severity,
        "candidate_violations": witness.candidate_violations,
    })));
    let mut direction = None;
    let class = if infrastructure {
        Class::Infrastructure
    } else if missing {
        Class::Unadjudicated
    } else if implied.is_some_and(|implied| implied != candidate) {
        // Helm's coalesced document and the screened composition disagree.
        Class::Unadjudicated
    } else {
        match oracle {
            // A collapsed flip: another identical execution answered it.
            OracleAnswer::NotAsked if record.is_some() => Class::Identical,
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
        case: witness.case.clone(),
        baseline_accepts: baseline,
        candidate_accepts: candidate,
        transition: witness.transition,
        oracle,
        class,
        direction,
        detail: if missing {
            Some("the live oracle returned no adjudication for this proposed witness".into())
        } else {
            detail
        },
        evidence,
        answer_sha256,
    }
}

/// Everything one comparison concluded, by witness.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutcomeTable {
    /// Every schema witness's outcome, sorted by witness id.
    pub outcomes: Vec<WitnessOutcome>,
    /// Every scored observation's outcome, sorted by witness id.
    pub observations: Vec<ObservationOutcome>,
    /// Every admission cell's outcome, sorted by witness id.
    pub admissions: Vec<AdmissionOutcome>,
}

impl OutcomeTable {
    /// The table of `outcomes`, sorted.
    #[must_use]
    pub fn new(mut outcomes: Vec<WitnessOutcome>) -> Self {
        outcomes.sort_by(|left, right| left.witness.cmp(&right.witness));
        Self {
            outcomes,
            observations: Vec::new(),
            admissions: Vec::new(),
        }
    }

    /// Every outcome of every table: schema witnesses, then observations,
    /// then admission cells.
    #[must_use]
    pub fn entries(&self) -> Vec<Entry<'_>> {
        let schema = self.outcomes.iter().map(|outcome| Entry {
            witness: &outcome.witness,
            case: &outcome.case,
            class: outcome.class,
            direction: outcome.direction.as_deref(),
            answer_sha256: &outcome.answer_sha256,
            detail: outcome.detail.as_deref(),
        });
        let observed = self.observations.iter().map(|outcome| Entry {
            witness: &outcome.witness,
            case: &outcome.case,
            class: outcome.class,
            direction: outcome.direction.as_deref(),
            answer_sha256: &outcome.answer_sha256,
            detail: None,
        });
        let admitted = self.admissions.iter().map(|outcome| Entry {
            witness: &outcome.witness,
            case: &outcome.cell,
            class: outcome.class,
            direction: outcome.direction.as_deref(),
            answer_sha256: &outcome.answer_sha256,
            detail: None,
        });
        schema.chain(observed).chain(admitted).collect()
    }

    /// The regression set.
    #[must_use]
    pub fn regressions(&self) -> BTreeSet<RegressionKey> {
        self.entries()
            .into_iter()
            .filter(|entry| entry.class == Class::Regression)
            .map(|entry| RegressionKey {
                witness: entry.witness.to_string(),
                direction: entry.direction.unwrap_or_default().to_string(),
                answer_sha256: entry.answer_sha256.to_string(),
            })
            .collect()
    }

    /// The earned closure set.
    #[must_use]
    pub fn closures(&self) -> BTreeSet<String> {
        self.entries()
            .into_iter()
            .filter(|entry| entry.class == Class::Closure)
            .map(|entry| entry.witness.to_string())
            .collect()
    }

    /// Outcomes per class.
    #[must_use]
    pub fn class_counts(&self) -> BTreeMap<Class, usize> {
        let mut counts = BTreeMap::new();
        for entry in self.entries() {
            *counts.entry(entry.class).or_insert(0) += 1;
        }
        counts
    }

    /// Why the table is not main compared with itself: every outcome must be
    /// IDENTICAL or an unchanged INHERITED obligation.
    #[must_use]
    pub fn inequalities(&self) -> Vec<String> {
        self.entries()
            .into_iter()
            .filter(|entry| !matches!(entry.class, Class::Identical | Class::Inherited))
            .map(|entry| format!("{} is {:?}", entry.case, entry.class))
            .collect()
    }

    /// The blocking outcomes: unadjudicated ones (semantic) and
    /// infrastructure ones.
    #[must_use]
    pub fn blocking(&self) -> PolicyVerdict {
        let mut verdict = PolicyVerdict::default();
        for entry in self.entries() {
            match entry.class {
                Class::Infrastructure => verdict.infrastructure.push(format!(
                    "{}: {}",
                    entry.case,
                    entry.detail.unwrap_or("infrastructure failure")
                )),
                Class::Unadjudicated => verdict
                    .semantic
                    .push(format!("UNADJUDICATED {}", entry.case)),
                Class::Identical
                | Class::Inherited
                | Class::Policy
                | Class::Closure
                | Class::Regression
                | Class::EquivalentChange => {}
            }
        }
        verdict
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
    /// The reviewed answer; a changed answer needs a fresh review.
    pub answer_sha256: String,
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
    let mut verdict = table.blocking();
    let regressions = table.regressions();
    let closures = table.closures();
    if *ordinal == 1 {
        for inequality in table.inequalities() {
            verdict
                .semantic
                .push(format!("checkpoint 1 must equal main, but {inequality}"));
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

/// The regression keys of reviewed `rows`.
#[must_use]
pub fn keys(rows: &[LedgerRow]) -> BTreeSet<RegressionKey> {
    rows.iter()
        .map(|row| RegressionKey {
            witness: row.witness.clone(),
            direction: row.direction.clone(),
            answer_sha256: row.answer_sha256.clone(),
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
        control: None,
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
                    WitnessMode::TemplateProbe | WitnessMode::Observation => {
                        return Err(DiffError::Identity(format!(
                            "witness {} is no document-lane witness",
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
        candidate_violations: Vec::new(),
    })
}
