//! The differential gate's commands: self-test, init, compare, amend and
//! verify; [`super::revision`] captures. Every path is explicit, every output
//! directory new, and every policy decision is made here, in typed code.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::admission::{self, AdmissionOutcome};
use super::build::dir_sha256;
use super::capture::{ArtifactPair, DumpPair, VerifiedDump};
use super::compare::{
    ConstructionRequest, LedgerRow, OutcomeTable, PriorSets, RegressionKey, SeedReview,
    construction_policy, keys,
};
use super::controls::{
    ControlBlobs, ControlCase, ControlResult, ObservationOutcome, classify_observation,
};
use super::history::{
    Attempt, BlobStore, ControlRecord, FORMAT_VERSION, GateHead, HistoryPlan, Receipt, ReceiptKind,
    Universe, UniverseState, WitnessMode, WitnessRecord, canonical_json, canonical_sha256,
    write_new,
};
use super::revision::{
    Capture, file_sha256, io, json_line, open_capture, replay_observation, write_lines,
};
use super::{
    ArtifactChange, ComparisonRun, DiffError, EquivalenceCertificate, RunOptions, artifact_changes,
    run_comparison,
};
use crate::acceptance::pool::PoolLimits;
use crate::acceptance::screen::ImportedRoster;
use crate::admission::SUITE_DIR;
use crate::helm::adjudication::{KUBERNETES_RELEASE, OfflineKubernetesValidator};
use crate::helm::invocation::{HelmRunner, RENDER_TIMEOUT};
use crate::registry::PROVIDER_BUNDLE;
use crate::source_digest::sha256_hex;

/// A command that ran to a verdict.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Complete and policy-satisfied: exit code 0.
    Pass,
    /// A semantic, unadjudicated or monotonicity failure: exit code 1.
    Fail(Vec<String>),
}

/// How live adjudication runs, shared by every comparing command.
pub struct LiveOptions {
    /// The pinned helmsweep; `None` judges with the schemas alone.
    pub helmsweep: Option<PathBuf>,
    /// The context roster, which must be the one both captures retained.
    pub contexts: Option<PathBuf>,
    /// The charts to compare; `None` compares every chart.
    pub selection: Option<BTreeSet<String>>,
    /// The run's private scratch directory.
    pub scratch: PathBuf,
}

/// Everything that makes two captures the same experiment: equal across a
/// comparison, and across a whole history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Experiment {
    /// The dump's registry entries and recipe inputs.
    pub suite: String,
    /// The admission suite.
    pub admission_suite: String,
    /// The provider bundle.
    pub provider: String,
    /// helmsweep, the pinned Helm and the Kubernetes release.
    pub helm: String,
    /// The context roster, when one was retained.
    pub contexts: Option<String>,
}

impl Experiment {
    /// The experiment `capture` ran.
    #[must_use]
    pub fn of(capture: &Capture) -> Self {
        let manifest = &capture.manifest;
        Self {
            suite: manifest.suite_sha256.clone(),
            admission_suite: manifest.admission_suite_sha256.clone(),
            provider: manifest.provider_sha256.clone(),
            helm: format!(
                "{} {} {}",
                manifest.helmsweep_sha256, manifest.helm_sha256, manifest.kubernetes_release
            ),
            contexts: manifest.contexts_sha256.clone(),
        }
    }

    /// The experiment a receipt recorded.
    #[must_use]
    pub fn recorded(receipt: &Receipt) -> Self {
        Self {
            suite: receipt.suite_hash.clone(),
            admission_suite: receipt.admission_suite_hash.clone(),
            provider: receipt.provider_hash.clone(),
            helm: receipt.helm_identity.clone(),
            contexts: receipt.context_roster_hash.clone(),
        }
    }
}

/// The Helm runner, validator and roster of one comparison.
struct LiveInputs {
    helm: Option<&'static HelmRunner>,
    kubernetes: Option<OfflineKubernetesValidator>,
    contexts: Option<ImportedRoster>,
}

/// The Helm runner, validator and roster for `options`, each required to be
/// the one `experiment` captured.
fn live_inputs(
    options: &LiveOptions,
    charts: &BTreeSet<String>,
    experiment: &Experiment,
    helmsweep_sha256: &str,
) -> Result<LiveInputs, DiffError> {
    let contexts_sha256 = options.contexts.as_deref().map(file_sha256).transpose()?;
    if contexts_sha256 != experiment.contexts {
        return Err(DiffError::Identity(
            "--contexts is not the roster both captures retained".into(),
        ));
    }
    let testdata = test_util::workspace_testdata();
    if dir_sha256(&testdata.join(PROVIDER_BUNDLE))? != experiment.provider {
        return Err(DiffError::Identity(
            "this checkout's provider bundle is not the captured one".into(),
        ));
    }
    let contexts = options
        .contexts
        .as_ref()
        .map(|path| ImportedRoster::read(path, charts))
        .transpose()
        .map_err(|error| DiffError::Request(format!("context roster: {error:#}")))?;
    let Some(helmsweep) = &options.helmsweep else {
        return Ok(LiveInputs {
            helm: None,
            kubernetes: None,
            contexts,
        });
    };
    if file_sha256(helmsweep)? != helmsweep_sha256 {
        return Err(DiffError::Identity(
            "--helmsweep is not the helmsweep both captures pinned".into(),
        ));
    }
    let runner = HelmRunner::with_helmsweep(
        &options.scratch.join("helm"),
        false,
        helmsweep.clone(),
        RENDER_TIMEOUT,
    )
    .map_err(|error| DiffError::Identity(format!("helmsweep: {error:#}")))?;
    let kubernetes = OfflineKubernetesValidator::with_crd_catalog(
        &testdata.join("provider-bundle/kubernetes-json-schema-cache"),
        &testdata.join("provider-bundle/crds-catalog-cache"),
        KUBERNETES_RELEASE,
    )
    .map_err(|error| DiffError::Infrastructure(format!("Kubernetes bundle: {error:#}")))?;
    Ok(LiveInputs {
        helm: Some(Box::leak(Box::new(runner))),
        kubernetes: Some(kubernetes),
        contexts,
    })
}

/// One comparison of two captures before any policy.
struct Comparison {
    run: ComparisonRun,
    /// Every artifact pair: the dump's, then the contract documents.
    artifacts: Vec<ArtifactPair>,
    /// Witnesses the universe does not hold, schema and observation.
    new_witnesses: Vec<WitnessRecord>,
    /// Failures after discovery; the caller journals first.
    failures: Vec<String>,
}

/// A comparison's observation witnesses.
struct ObservationCases {
    /// Every case by witness id.
    cases: BTreeMap<String, ControlCase>,
    /// Records of the cases the universe does not hold.
    new: Vec<WitnessRecord>,
    /// How many grounding-only cases the registries hold.
    grounding_only: usize,
}

/// The observation witnesses of a comparison: the universe's and both
/// registries' scored cases, the new ones as records whose blobs are stored.
fn observation_cases(
    main: &Capture,
    candidate: &Capture,
    universe: &UniverseState,
    blobs: &BlobStore,
    origin: &str,
) -> Result<ObservationCases, DiffError> {
    let mut cases = BTreeMap::new();
    for record in universe
        .witnesses
        .values()
        .filter(|record| record.identity.mode == WitnessMode::Observation)
    {
        let control = record.control.as_ref().ok_or_else(|| {
            DiffError::Identity(format!("observation witness {} has no case", record.id))
        })?;
        cases.insert(record.id.clone(), control.case(&record.identity, blobs)?);
    }
    let mut new = Vec::new();
    let mut grounding_only = BTreeSet::new();
    for capture in [main, candidate] {
        let source = capture.controls.registry.blobs();
        for case in capture.controls.registry.cases.values() {
            if case.scored_query().is_none() {
                grounding_only.insert(case.id.clone());
                continue;
            }
            let id = case.witness_id()?;
            if let Some(existing) = cases.get(&id) {
                if canonical_case(existing)? != canonical_case(case)? {
                    return Err(DiffError::Identity(format!(
                        "control {} redefines observation witness {id}; migrate the history",
                        case.id
                    )));
                }
                continue;
            }
            new.push(observation_record(case, &source, blobs, origin)?);
            cases.insert(id, case.clone());
        }
    }
    Ok(ObservationCases {
        cases,
        new,
        grounding_only: grounding_only.len(),
    })
}

fn canonical_case(case: &ControlCase) -> Result<Vec<u8>, DiffError> {
    serde_json::to_value(case)
        .map(|value| canonical_json(&value))
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))
}

/// The persistent record of a scored control, its case and closure stored
/// in `blobs`.
fn observation_record(
    case: &ControlCase,
    source: &ControlBlobs,
    blobs: &BlobStore,
    origin: &str,
) -> Result<WitnessRecord, DiffError> {
    let closure = case.closure(source)?;
    for blob in &closure {
        if blobs.put(&source.read(blob)?)? != *blob {
            return Err(DiffError::Identity(format!(
                "blob {blob} rehashed differently"
            )));
        }
    }
    let identity = case.identity()?;
    Ok(WitnessRecord {
        format_version: FORMAT_VERSION,
        id: identity.id()?,
        name: case.id.clone(),
        overlay: Value::Null,
        values_blob: case.input.original_values_sha256.clone(),
        composed_sha256: case.input.coalesced_document_sha256.clone(),
        defaults_blob: None,
        instance: None,
        document: None,
        control: Some(ControlRecord {
            case_blob: blobs.put(&canonical_case(case)?)?,
            blobs: closure.into_iter().collect(),
        }),
        origins: vec![origin.to_string()],
        identity,
    })
}

/// Classifies every observation witness: each capture answers from its own
/// record when it has one, else its retained runner replays the case.
fn observation_outcomes(
    main: &Capture,
    candidate: &Capture,
    cases: &BTreeMap<String, ControlCase>,
    blobs: &BlobStore,
    scratch: &Path,
) -> (Vec<ObservationOutcome>, Vec<String>) {
    let source = ControlBlobs::store(blobs.root());
    let answer = |capture: &Capture, id: &str, case: &ControlCase, run: PathBuf| match capture
        .controls
        .observations
        .get(id)
    {
        Some(record) => Ok(record.response.result.clone()),
        None => {
            replay_observation(capture, &source, case, &run).map(|record| record.response.result)
        }
    };
    let mut outcomes = Vec::new();
    let mut failures = Vec::new();
    for (index, (id, case)) in cases.iter().enumerate() {
        let answers: Result<(ControlResult, ControlResult), DiffError> =
            answer(main, id, case, scratch.join(format!("main-{index:04}"))).and_then(|left| {
                answer(
                    candidate,
                    id,
                    case,
                    scratch.join(format!("candidate-{index:04}")),
                )
                .map(|right| (left, right))
            });
        match answers.and_then(|(left, right)| classify_observation(case, &left, &right)) {
            Ok(outcome) => outcomes.push(outcome),
            Err(error) => failures.push(format!("control {}: {error}", case.id)),
        }
    }
    outcomes.sort_by(|left, right| left.witness.cmp(&right.witness));
    (outcomes, failures)
}

/// The contract-document pairs of two captures.
fn contract_artifacts(main: &Capture, candidate: &Capture) -> Result<Vec<ArtifactPair>, DiffError> {
    if main.contracts.keys().ne(candidate.contracts.keys()) {
        return Err(DiffError::Identity(
            "the captures exported different contract documents".into(),
        ));
    }
    Ok(main
        .contracts
        .iter()
        .zip(candidate.contracts.values())
        .map(|((key, left), right)| ArtifactPair {
            key: format!("contract/{key}"),
            kind: "contract".into(),
            baseline_sha256: left.sha256.clone(),
            candidate_sha256: right.sha256.clone(),
        })
        .collect())
}

/// Compares two captures over `universe` and fresh discovery, without any
/// policy and without journaling.
fn compare_captures(
    main: &Capture,
    candidate: &Capture,
    universe: UniverseState,
    blobs: &BlobStore,
    options: &LiveOptions,
    origin: &str,
) -> Result<Comparison, DiffError> {
    let experiment = Experiment::of(main);
    if Experiment::of(candidate) != experiment {
        return Err(DiffError::Identity(
            "the captures ran different experiments; migrate the history".into(),
        ));
    }
    let testdata = test_util::workspace_testdata();
    if dir_sha256(&testdata.join(SUITE_DIR))? != experiment.admission_suite {
        return Err(DiffError::Identity(
            "this checkout's admission suite is not the captured one".into(),
        ));
    }
    let suite = admission::read_checked_suite(&testdata.join(SUITE_DIR))?;
    let admissions = admission::compare(&suite, &main.admissions, &candidate.admissions)?;
    let pair = DumpPair::new(
        VerifiedDump::open(main.dump.root())?,
        VerifiedDump::open(candidate.dump.root())?,
        &testdata,
    )?;
    let labels = super::capture::chart_keys(&pair.baseline)?
        .into_keys()
        .filter(|label| {
            options
                .selection
                .as_ref()
                .is_none_or(|selection| selection.contains(label))
        })
        .collect::<BTreeSet<_>>();
    let LiveInputs {
        helm,
        kubernetes,
        contexts,
    } = live_inputs(
        options,
        &labels,
        &experiment,
        &main.manifest.helmsweep_sha256,
    )?;
    let ObservationCases {
        cases,
        new: observed_new,
        grounding_only,
    } = observation_cases(main, candidate, &universe, blobs, origin)?;
    let mut run = run_comparison(
        &pair,
        universe,
        blobs,
        RunOptions {
            kubernetes,
            contexts,
            pool: PoolLimits::from_env()
                .map_err(|error| DiffError::Request(format!("pool limits: {error:#}")))?,
            selection: options.selection.clone(),
            origin: origin.to_string(),
            helm,
        },
    )?;
    let (observations, observation_failures) = observation_outcomes(
        main,
        candidate,
        &cases,
        blobs,
        &options.scratch.join("observations"),
    );
    if let Some(coverage) = run.coverage.as_object_mut() {
        coverage.insert(
            "observations".into(),
            super::controls::coverage(&observations, grounding_only),
        );
        coverage.insert("admission_cells".into(), json!(admissions.len()));
    }
    run.table.observations = observations;
    run.table.admissions = admissions;
    let mut failures = std::mem::take(&mut run.failures);
    failures.extend(observation_failures);
    let mut new_witnesses = std::mem::take(&mut run.new_witnesses);
    new_witnesses.extend(observed_new);
    let mut artifacts = pair.artifacts();
    artifacts.extend(contract_artifacts(main, candidate)?);
    Ok(Comparison {
        run,
        artifacts,
        new_witnesses,
        failures,
    })
}

/// The bytes of both sides of a changed artifact.
fn artifact_bytes(
    main: &Capture,
    candidate: &Capture,
    artifact: &ArtifactPair,
) -> Result<(Vec<u8>, Vec<u8>), DiffError> {
    let read = |capture: &Capture| -> Result<Vec<u8>, DiffError> {
        match artifact.key.strip_prefix("contract/") {
            Some(key) => {
                let contract = capture
                    .contracts
                    .get(key)
                    .ok_or_else(|| DiffError::Identity(format!("{key}: no contract document")))?;
                let path = capture.root.join("export").join(&contract.file);
                std::fs::read(&path).map_err(|error| io(&path, &error))
            }
            None => Ok(capture.dump.schema(&artifact.key)?.bytes),
        }
    };
    Ok((read(main)?, read(candidate)?))
}

/// The changed artifacts of a comparison, each accounted for only by a
/// certificate.
fn changes_of(
    main: &Capture,
    candidate: &Capture,
    comparison: &Comparison,
    certificates: &[EquivalenceCertificate],
    reviewer: Option<&str>,
) -> Result<Vec<ArtifactChange>, DiffError> {
    artifact_changes(
        &comparison.artifacts,
        &|artifact| artifact_bytes(main, candidate, artifact),
        &comparison.run.table,
        certificates,
        reviewer,
    )
}

/// The files a comparison wrote, by table name, and the payload index.
struct Tables {
    hashes: BTreeMap<&'static str, String>,
    payload_sha256: String,
}

/// Copies each outcome's adjudication evidence directory into
/// `out/evidence/<witness>` and points the outcome at the copy, so no
/// receipt depends on swept scratch.
fn retain_evidence(out: &Path, table: &mut OutcomeTable) -> Result<(), DiffError> {
    for outcome in &mut table.outcomes {
        let Some(detail) = &outcome.detail else {
            continue;
        };
        let source = PathBuf::from(detail);
        if !source.is_absolute() || !source.is_dir() {
            continue;
        }
        let relative = format!("evidence/{}", outcome.witness);
        copy_tree(&source, &out.join(&relative))?;
        outcome.detail = Some(relative);
    }
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), DiffError> {
    std::fs::create_dir_all(target).map_err(|error| io(target, &error))?;
    for entry in std::fs::read_dir(source).map_err(|error| io(source, &error))? {
        let entry = entry.map_err(|error| io(source, &error))?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        let kind = entry.file_type().map_err(|error| io(&path, &error))?;
        if kind.is_dir() {
            copy_tree(&path, &destination)?;
        } else if kind.is_file() {
            std::fs::copy(&path, &destination).map_err(|error| io(&path, &error))?;
        }
    }
    Ok(())
}

/// A comparison's payload index: every file it published.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadIndex {
    /// [`FORMAT_VERSION`].
    format_version: u32,
    /// The files, sorted by path.
    files: Vec<PayloadFile>,
}

/// One published file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadFile {
    /// Its path relative to the payload, `/`-separated.
    path: String,
    /// SHA-256 of its bytes.
    sha256: String,
}

/// Parses a payload index, refusing another format or an escaping path.
fn read_payload(bytes: &[u8]) -> Result<PayloadIndex, DiffError> {
    let index: PayloadIndex = serde_json::from_slice(bytes)
        .map_err(|error| DiffError::Identity(format!("payload.json: {error}")))?;
    if index.format_version != FORMAT_VERSION
        || index
            .files
            .iter()
            .any(|file| !super::controls::is_normal_relative(&file.path))
    {
        return Err(DiffError::Identity("payload.json is malformed".into()));
    }
    Ok(index)
}

/// Writes the payload index of every file under `out` except `scratch/`.
fn write_payload(out: &Path) -> Result<String, DiffError> {
    let mut files = Vec::new();
    let mut pending = vec![out.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|error| io(&dir, &error))? {
            let path = entry.map_err(|error| io(&dir, &error))?.path();
            let relative = path
                .strip_prefix(out)
                .map_err(|error| DiffError::Infrastructure(error.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            if relative == "scratch" || relative == "payload.json" {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(PayloadFile {
                    sha256: file_sha256(&path)?,
                    path: relative,
                });
            }
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let index = serde_json::to_value(PayloadIndex {
        format_version: FORMAT_VERSION,
        files,
    })
    .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    let bytes = canonical_json(&index);
    write_new(&out.join("payload.json"), &bytes)?;
    Ok(sha256_hex(&bytes))
}

/// Writes the outcome tables of `comparison` into the new directory `out`,
/// retaining evidence, then the payload index.
fn write_tables(
    out: &Path,
    comparison: &mut Comparison,
    changes: &[ArtifactChange],
) -> Result<Tables, DiffError> {
    let run = &mut comparison.run;
    retain_evidence(out, &mut run.table)?;
    let mut hashes = BTreeMap::new();
    let mut table = |name: &'static str, file: &str, lines: Vec<String>| -> Result<(), DiffError> {
        hashes.insert(name, write_lines(&out.join(file), &lines)?);
        Ok(())
    };
    table(
        "outcomes",
        "outcomes.jsonl",
        jsonl(run.table.outcomes.iter())?,
    )?;
    table(
        "observations",
        "observations.jsonl",
        jsonl(run.table.observations.iter())?,
    )?;
    table(
        "admissions",
        "admissions.jsonl",
        jsonl(run.table.admissions.iter())?,
    )?;
    table(
        "artifacts",
        "artifacts.jsonl",
        jsonl(comparison.artifacts.iter())?,
    )?;
    table("changes", "changes.jsonl", jsonl(changes.iter())?)?;
    table(
        "regressions",
        "regressions.jsonl",
        jsonl(run.table.regressions().iter())?,
    )?;
    table(
        "closures",
        "closures.tsv",
        run.table.closures().into_iter().collect(),
    )?;
    let mut coverage = run.coverage.clone();
    if let Some(members) = coverage.as_object_mut() {
        members.insert("artifact_changes".into(), json!(changes.len()));
        members.insert("failures".into(), json!(comparison.failures));
    }
    let coverage = serde_json::to_vec_pretty(&coverage)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    write_new(&out.join("coverage.json"), &coverage)?;
    hashes.insert("coverage", sha256_hex(&coverage));
    Ok(Tables {
        hashes,
        payload_sha256: String::new(),
    })
}

/// Seals the payload of `out` after every file is written.
fn seal_payload(out: &Path, tables: &mut Tables) -> Result<(), DiffError> {
    tables.payload_sha256 = write_payload(out)?;
    Ok(())
}

/// Problems with changed artifacts no certificate accounts for.
fn unadjudicated_changes(changes: &[ArtifactChange]) -> Vec<String> {
    changes
        .iter()
        .filter(|change| !change.adjudicated)
        .map(|change| {
            format!(
                "UNADJUDICATED artifact change {} at {:?}: {}",
                change.artifact.key,
                change.pointers.iter().take(8).collect::<Vec<_>>(),
                change.problems.join("; ")
            )
        })
        .collect()
}

/// One canonical JSON line per item, serialized one at a time.
fn jsonl<T: Serialize>(items: impl IntoIterator<Item = T>) -> Result<Vec<String>, DiffError> {
    items.into_iter().map(|item| json_line(&item)).collect()
}

/// Reports infrastructure `problems` (exit code 3) and fails with them.
fn infrastructure(out: &Path, problems: &[String]) -> Result<Verdict, DiffError> {
    write_report(out, 3, problems)?;
    Err(DiffError::Infrastructure(problems.join("; ")))
}

fn write_report(out: &Path, exit_code: u8, problems: &[String]) -> Result<(), DiffError> {
    let report = json!({"exit_code": exit_code, "problems": problems, "receipt": null});
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    write_new(&out.join("report.json"), &bytes)
}

fn new_dir(out: &Path) -> Result<(), DiffError> {
    if out.exists() {
        return Err(DiffError::Request(format!(
            "{} already exists",
            out.display()
        )));
    }
    std::fs::create_dir_all(out).map_err(|error| io(out, &error))
}

/// Compares two captures without any history: the main==main self-test
/// and diagnostic comparisons. It never writes a receipt.
///
/// # Errors
///
/// [`DiffError`] when an input is invalid or the comparison cannot run;
/// [`DiffError::Infrastructure`] when it failed after writing its tables.
pub fn self_test(
    main: &Path,
    candidate: &Path,
    out: &Path,
    options: &LiveOptions,
    certificates: &[EquivalenceCertificate],
) -> Result<Verdict, DiffError> {
    new_dir(out)?;
    let main = open_capture(main)?;
    let candidate = open_capture(candidate)?;
    let blobs = BlobStore::new(options.scratch.join("blobs"));
    let mut comparison = compare_captures(
        &main,
        &candidate,
        UniverseState::default(),
        &blobs,
        options,
        "attempt:self-test",
    )?;
    let changes = changes_of(&main, &candidate, &comparison, certificates, None)?;
    write_tables(out, &mut comparison, &changes)?;
    let counts = comparison.run.table.class_counts();
    let mut problems = comparison.run.table.inequalities();
    problems.extend(unadjudicated_changes(&changes));
    if comparison.run.witness_count == 0 || comparison.run.table.observations.is_empty() {
        problems.push("vacuous: no schema witness or observation was judged".to_string());
    }
    problems.extend(comparison.failures.iter().cloned());
    let report = json!({
        "mode": "self-test",
        "main": main.hash, "candidate": candidate.hash,
        "witnesses": comparison.run.witness_count,
        "observations": comparison.run.table.observations.len(),
        "admission_cells": comparison.run.table.admissions.len(),
        "classes": counts.iter().map(|(class, count)| (format!("{class:?}"), count)).collect::<BTreeMap<_, _>>(),
        "artifact_changes": changes.len(),
        "problems": problems,
    });
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    write_new(&out.join("report.json"), &bytes)?;
    if !comparison.failures.is_empty() {
        return Err(DiffError::Infrastructure(comparison.failures.join("; ")));
    }
    Ok(if problems.is_empty() {
        Verdict::Pass
    } else {
        Verdict::Fail(problems)
    })
}

/// Paths of one history.
struct History {
    root: PathBuf,
    universe: Universe,
}

impl History {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            universe: Universe::new(root.join("universe")),
        }
    }

    fn plan(&self) -> Result<(HistoryPlan, String), DiffError> {
        let path = self.root.join("plan.json");
        Ok((HistoryPlan::read(&path)?, file_sha256(&path)?))
    }

    fn gate_head(&self) -> Result<GateHead, DiffError> {
        let path = self.root.join("gate-head.json");
        let bytes = std::fs::read(&path).map_err(|error| io(&path, &error))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))
    }

    fn tables_dir(&self, seal: &str) -> PathBuf {
        self.root.join("tables").join(seal)
    }

    fn payload_dir(&self, payload: &str) -> PathBuf {
        self.root.join("payloads").join(payload)
    }

    /// The prior sets a trusted receipt recorded, rehashed against it.
    fn prior_sets(&self, receipt: &Receipt) -> Result<PriorSets, DiffError> {
        let path = self.tables_dir(&receipt.seal_sha256).join("sets.json");
        let bytes = std::fs::read(&path).map_err(|error| io(&path, &error))?;
        let sets: PriorSets = serde_json::from_slice(&bytes)
            .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))?;
        if canonical_sha256(&sets.regressions)? != receipt.regression_set_hash
            || canonical_sha256(&sets.closures)? != receipt.earned_closure_set_hash
            || sets.ordinal != receipt.checkpoint_ordinal
        {
            return Err(DiffError::Identity(format!(
                "{} does not match its receipt",
                path.display()
            )));
        }
        Ok(sets)
    }

    /// Publishes `receipt`, its sets, its payload copied from `out` and its
    /// capture under the history.
    fn publish(
        &self,
        receipt: &Receipt,
        sets: &PriorSets,
        out: &Path,
        capture: Option<&Capture>,
    ) -> Result<PathBuf, DiffError> {
        let tables = self.tables_dir(&receipt.seal_sha256);
        let bytes = serde_json::to_vec_pretty(sets)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
        write_new(&tables.join("sets.json"), &bytes)?;
        let payload = self.payload_dir(&receipt.payload_index_sha256);
        if !payload.exists() {
            copy_payload(out, &payload)?;
        }
        let path = self.root.join("receipts").join(format!(
            "{:02}-{:?}-{}.json",
            receipt.checkpoint_ordinal, receipt.kind, receipt.seal_sha256
        ));
        receipt.write(&path)?;
        if let Some(capture) = capture {
            let line = json_line(&json!({
                "ordinal": receipt.checkpoint_ordinal,
                "capture_hash": capture.hash,
                "path": capture.root,
                "receipt": receipt.seal_sha256,
            }))?;
            let registry = self.root.join("captures.jsonl");
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&registry)
                .map_err(|error| io(&registry, &error))?;
            writeln!(file, "{line}").map_err(|error| io(&registry, &error))?;
        }
        Ok(path)
    }

    /// Advances the trusted gate head to `receipt` at `path`.
    fn advance(&self, receipt: &Receipt, path: &Path) -> Result<(), DiffError> {
        let head = GateHead {
            format_version: FORMAT_VERSION,
            seal_sha256: receipt.seal_sha256.clone(),
            receipt: path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned(),
        };
        let bytes = serde_json::to_vec_pretty(&head)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
        let next = self
            .root
            .join(format!("gate-head.{}.json", receipt.seal_sha256));
        write_new(&next, &bytes)?;
        std::fs::rename(&next, self.root.join("gate-head.json"))
            .map_err(|error| io(&self.root, &error))
    }
}

/// Copies the payload files `out/payload.json` lists into `target`.
fn copy_payload(out: &Path, target: &Path) -> Result<(), DiffError> {
    let index = std::fs::read(out.join("payload.json")).map_err(|error| io(out, &error))?;
    write_new(&target.join("payload.json"), &index)?;
    for file in read_payload(&index)?.files {
        let bytes = std::fs::read(out.join(&file.path)).map_err(|error| io(out, &error))?;
        write_new(&target.join(&file.path), &bytes)?;
    }
    Ok(())
}

/// This tool's identity: its version and the hash of its executable.
fn tool_identity() -> Result<String, DiffError> {
    let executable = std::env::current_exe()
        .map_err(|error| DiffError::Infrastructure(format!("locate this executable: {error}")))?;
    Ok(format!(
        "value_algebra_diff {} {}",
        env!("CARGO_PKG_VERSION"),
        file_sha256(&executable)?
    ))
}

fn run_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("{nanos}-{}", std::process::id())
}

/// Everything a receipt binds besides its kind, ordinal and sets.
struct ReceiptInputs<'a> {
    plan: &'a HistoryPlan,
    plan_sha256: &'a str,
    main: &'a Capture,
    candidate: Option<&'a Capture>,
    predecessor: Option<&'a Receipt>,
    universe: &'a UniverseState,
    tables: &'a Tables,
    seed_ledger_sha256: Option<String>,
}

fn receipt(
    inputs: &ReceiptInputs<'_>,
    kind: ReceiptKind,
    ordinal: u32,
    sets: &PriorSets,
) -> Result<Receipt, DiffError> {
    let ReceiptInputs {
        plan,
        plan_sha256,
        main,
        candidate,
        predecessor,
        universe,
        tables,
        seed_ledger_sha256,
    } = inputs;
    let hash = |name: &str| tables.hashes.get(name).cloned().unwrap_or_default();
    let experiment = Experiment::of(main);
    let runners = [Some(*main), *candidate]
        .into_iter()
        .flatten()
        .map(|capture| {
            [
                capture.manifest.controls.runner.clone(),
                capture.manifest.build_sha256.clone(),
            ]
        })
        .collect::<Vec<_>>();
    Receipt {
        format_version: FORMAT_VERSION,
        tool_version_and_hash: tool_identity()?,
        run_id: run_id(),
        history_id: plan.history_id.clone(),
        kind,
        checkpoint_ordinal: ordinal,
        checkpoint_commit: candidate.map(|capture| capture.manifest.commit.clone()),
        capture_hash: candidate.map(|capture| capture.hash.clone()),
        baseline_capture_hash: main.hash.clone(),
        suite_hash: experiment.suite,
        controls_hash: Some(main.manifest.controls.registry.clone()),
        context_roster_hash: experiment.contexts,
        provider_hash: experiment.provider,
        helm_identity: experiment.helm,
        predecessor_receipt_sha256: predecessor.map(|receipt| receipt.seal_sha256.clone()),
        seed_ledger_sha256: seed_ledger_sha256.clone(),
        universe_parent_hash: predecessor.map(|receipt| receipt.universe_hash.clone()),
        universe_hash: universe.hash()?,
        universe_journal_head_sha256: universe.head.clone(),
        witness_batch_hashes: universe.batch_hashes(),
        artifact_manifest_hashes: [Some(*main), *candidate]
            .into_iter()
            .flatten()
            .map(|capture| capture.manifest.dump_manifest_sha256.clone())
            .collect(),
        coverage_hash: hash("coverage"),
        proof_index_hash: hash("changes"),
        regression_set_hash: canonical_sha256(&sets.regressions)?,
        earned_closure_set_hash: canonical_sha256(&sets.closures)?,
        full_outcome_table_hash: hash("outcomes"),
        control_runner_manifest_hash: Some(canonical_sha256(&runners)?),
        observation_results_hash: Some(hash("observations")),
        admission_suite_hash: experiment.admission_suite,
        plan_sha256: (*plan_sha256).to_string(),
        payload_index_sha256: tables.payload_sha256.clone(),
        gate_mode: "construction".into(),
        landability: "NON-LANDABLE".into(),
        exit_code: 0,
        seal_sha256: String::new(),
    }
    .seal()
}

/// Journals `witnesses` as one batch; returns its hash when it grew.
fn journal(
    history: &History,
    witnesses: Vec<WitnessRecord>,
    capture: &Capture,
    reason: &str,
) -> Result<(UniverseState, Option<String>), DiffError> {
    let lock = history.universe.lock()?;
    let mut universe = history.universe.load()?;
    let grown = history.universe.append(
        &lock,
        &mut universe,
        witnesses,
        Attempt {
            run_id: run_id(),
            capture_hash: Some(capture.hash.clone()),
            reason: reason.into(),
        },
    )?;
    drop(lock);
    Ok((universe, grown))
}

/// Creates a history: main compared with itself discovers the first
/// universe batch and seals the genesis receipt at ordinal 0. Main must
/// equal itself up to unchanged inherited obligations and measure the
/// admission suite exactly as recorded.
///
/// # Errors
///
/// [`DiffError`] when an input is invalid, main is synthetic, or main does
/// not compare equally with itself.
pub fn init(
    main: &Path,
    plan: &Path,
    out: &Path,
    options: &LiveOptions,
) -> Result<Verdict, DiffError> {
    new_dir(out)?;
    if options.helmsweep.is_none() || options.selection.is_some() {
        return Err(DiffError::Request(
            "genesis adjudicates every chart live; pass --helmsweep and no --chart".into(),
        ));
    }
    let main = open_capture(main)?;
    if main.manifest.synthetic {
        return Err(DiffError::Identity("main capture is synthetic".into()));
    }
    let plan_value = HistoryPlan::read(plan)?;
    if plan_value.main_commit != main.manifest.commit {
        return Err(DiffError::Identity(format!(
            "the plan pins main {}, the capture is {}",
            plan_value.main_commit, main.manifest.commit
        )));
    }
    let suite = admission::read_checked_suite(&test_util::workspace_testdata().join(SUITE_DIR))?;
    let disagreements = admission::main_disagreements(&suite, &main.admissions);
    if !disagreements.is_empty() {
        return Err(DiffError::Identity(disagreements.join("; ")));
    }
    let history = History::new(out);
    write_new(
        &out.join("plan.json"),
        &std::fs::read(plan).map_err(|error| io(plan, &error))?,
    )?;
    let (_, plan_sha256) = history.plan()?;
    let universe = history.universe.load()?;
    let mut comparison = compare_captures(
        &main,
        &main,
        universe,
        history.universe.blobs(),
        options,
        "attempt:genesis",
    )?;
    let (universe, _) = journal(
        &history,
        std::mem::take(&mut comparison.new_witnesses),
        &main,
        "genesis",
    )?;
    let tables_out = out.join("genesis");
    std::fs::create_dir_all(&tables_out).map_err(|error| io(&tables_out, &error))?;
    let changes = changes_of(&main, &main, &comparison, &[], None)?;
    let mut tables = write_tables(&tables_out, &mut comparison, &changes)?;
    if !comparison.failures.is_empty() {
        return Err(DiffError::Infrastructure(comparison.failures.join("; ")));
    }
    let mut problems = comparison.run.table.inequalities();
    if !changes.is_empty() || comparison.run.witness_count == 0 {
        problems.push(format!(
            "main does not compare equally with itself: {} changes, {} witnesses",
            changes.len(),
            comparison.run.witness_count
        ));
    }
    if !problems.is_empty() {
        return Err(DiffError::Infrastructure(problems.join("; ")));
    }
    seal_payload(&tables_out, &mut tables)?;
    let sets = PriorSets::default();
    let genesis = receipt(
        &ReceiptInputs {
            plan: &plan_value,
            plan_sha256: &plan_sha256,
            main: &main,
            candidate: None,
            predecessor: None,
            universe: &universe,
            tables: &tables,
            seed_ledger_sha256: None,
        },
        ReceiptKind::Genesis,
        0,
        &sets,
    )?;
    let path = history.publish(&genesis, &sets, &tables_out, Some(&main))?;
    history.advance(&genesis, &path)?;
    Ok(Verdict::Pass)
}

/// What `compare` needs.
pub struct CompareRequest {
    /// Main's capture.
    pub main: PathBuf,
    /// The checkpoint's capture.
    pub candidate: PathBuf,
    /// The history.
    pub history: PathBuf,
    /// The trusted predecessor receipt.
    pub previous: PathBuf,
    /// The regression ledger, a JSON list of rows.
    pub ledger: PathBuf,
    /// The new output directory.
    pub out: PathBuf,
    /// The seed review, only at the seed ordinal.
    pub seed_review: Option<PathBuf>,
    /// Reviewed certificates for changed artifacts.
    pub certificates: Vec<EquivalenceCertificate>,
    /// Live adjudication.
    pub live: LiveOptions,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, DiffError> {
    let bytes = std::fs::read(path).map_err(|error| io(path, &error))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| DiffError::Request(format!("{}: {error}", path.display())))
}

/// The trusted predecessor: `previous` must be the history's gate head,
/// made by this very tool under the sealed plan.
fn trusted_predecessor(
    history: &History,
    previous: &Path,
    plan_sha256: &str,
) -> Result<Receipt, DiffError> {
    let receipt = Receipt::read(previous)?;
    let head = history.gate_head()?;
    if head.seal_sha256 != receipt.seal_sha256 {
        return Err(DiffError::Identity(format!(
            "{} is not the trusted gate head {}",
            previous.display(),
            head.seal_sha256
        )));
    }
    if receipt.plan_sha256 != plan_sha256 {
        return Err(DiffError::Identity(
            "the history's plan changed after it was sealed".into(),
        ));
    }
    if receipt.tool_version_and_hash != tool_identity()? {
        return Err(DiffError::Identity(format!(
            "the predecessor was made by {}; a changed tool needs a migration",
            receipt.tool_version_and_hash
        )));
    }
    Ok(receipt)
}

/// Everything a checkpoint comparison checked before comparing.
struct Checkpoint {
    history: History,
    plan: HistoryPlan,
    plan_sha256: String,
    predecessor: Receipt,
    main: Capture,
    candidate: Capture,
    ordinal: u32,
    semantic: bool,
    prior: PriorSets,
    ledger: Vec<LedgerRow>,
    seed: Option<SeedReview>,
}

/// Checks a compare request's inputs, chain and experiment identity.
fn checkpoint(request: &CompareRequest) -> Result<Checkpoint, DiffError> {
    let CompareRequest {
        main,
        candidate,
        history,
        previous,
        ledger,
        out,
        seed_review,
        certificates: _,
        live,
    } = request;
    if out.exists() {
        return Err(DiffError::Request(format!(
            "{} already exists",
            out.display()
        )));
    }
    if live.selection.is_some() || live.helmsweep.is_none() {
        return Err(DiffError::Request(
            "a construction comparison adjudicates every chart live; pass --helmsweep and no --chart"
                .into(),
        ));
    }
    let history = History::new(history);
    let (plan, plan_sha256) = history.plan()?;
    let predecessor = trusted_predecessor(&history, previous, &plan_sha256)?;
    let main = open_capture(main)?;
    let candidate = open_capture(candidate)?;
    if predecessor.history_id != plan.history_id || predecessor.baseline_capture_hash != main.hash {
        return Err(DiffError::Identity(
            "the predecessor belongs to another history or main".into(),
        ));
    }
    if Experiment::recorded(&predecessor) != Experiment::of(&candidate) {
        return Err(DiffError::Identity(
            "the checkpoint ran another experiment than its history; migrate".into(),
        ));
    }
    if candidate.manifest.synthetic {
        return Err(DiffError::Identity(
            "a synthetic capture cannot enter a history".into(),
        ));
    }
    let ordinal = predecessor.checkpoint_ordinal + 1;
    let semantic = plan.checkpoint(ordinal)?.kind == super::history::CheckpointKind::Semantic;
    let prior = history.prior_sets(&predecessor)?;
    Ok(Checkpoint {
        ledger: read_json(ledger)?,
        seed: seed_review.as_deref().map(read_json).transpose()?,
        history,
        plan,
        plan_sha256,
        predecessor,
        main,
        candidate,
        ordinal,
        semantic,
        prior,
    })
}

/// Compares one construction checkpoint with main over the persistent
/// universe and applies the construction policy. Fresh discoveries are
/// journaled before anything else can fail; a grown universe stops the
/// comparison until a reviewed amendment backfills history. A passing
/// comparison publishes its receipt and advances the gate head.
///
/// # Errors
///
/// [`DiffError`] when an input, identity or chain is invalid or the
/// comparison cannot run.
pub fn compare(request: &CompareRequest) -> Result<Verdict, DiffError> {
    let Checkpoint {
        history,
        plan,
        plan_sha256,
        predecessor,
        main,
        candidate,
        ordinal,
        semantic,
        prior,
        ledger,
        seed,
    } = checkpoint(request)?;
    let out = &request.out;
    new_dir(out)?;
    let universe = history.universe.load()?;
    if universe.head != predecessor.universe_journal_head_sha256 {
        return Err(DiffError::Identity(
            "the universe moved past the trusted predecessor; amend first".into(),
        ));
    }
    let mut comparison = compare_captures(
        &main,
        &candidate,
        universe,
        history.universe.blobs(),
        &request.live,
        &format!("attempt:checkpoint-{ordinal}"),
    )?;
    let discovered = comparison.new_witnesses.len();
    let (universe, grown) = journal(
        &history,
        std::mem::take(&mut comparison.new_witnesses),
        &candidate,
        "discovery",
    )?;
    let changes = changes_of(
        &main,
        &candidate,
        &comparison,
        &request.certificates,
        Some(&plan.reviewer),
    )?;
    let mut tables = write_tables(out, &mut comparison, &changes)?;
    if !comparison.failures.is_empty() {
        return infrastructure(out, &comparison.failures);
    }
    let mut problems = unadjudicated_changes(&changes);
    if let Some(batch) = &grown {
        problems.push(format!(
            "the universe grew by {discovered} witnesses (batch {batch}); backfill history with amend"
        ));
    }
    let policy = construction_policy(&ConstructionRequest {
        ordinal,
        semantic,
        table: &comparison.run.table,
        prior: &prior,
        ledger: &ledger,
        seed: seed.as_ref(),
        seed_ordinal: plan.seed_ordinal,
        reviewer: &plan.reviewer,
    })?;
    if !policy.infrastructure.is_empty() {
        return infrastructure(out, &policy.infrastructure);
    }
    problems.extend(policy.semantic);
    if !problems.is_empty() {
        write_report(out, 1, &problems)?;
        return Ok(Verdict::Fail(problems));
    }
    seal_payload(out, &mut tables)?;
    let sets = PriorSets {
        ordinal,
        regressions: comparison.run.table.regressions(),
        closures: comparison.run.table.closures(),
    };
    let seed_ledger_sha256 = match &seed {
        Some(seed) => Some(canonical_sha256(seed)?),
        None => predecessor.seed_ledger_sha256.clone(),
    };
    let sealed = receipt(
        &ReceiptInputs {
            plan: &plan,
            plan_sha256: &plan_sha256,
            main: &main,
            candidate: Some(&candidate),
            predecessor: Some(&predecessor),
            universe: &universe,
            tables: &tables,
            seed_ledger_sha256,
        },
        ReceiptKind::Checkpoint,
        ordinal,
        &sets,
    )?;
    sealed.write(&out.join("receipt.json"))?;
    let path = history.publish(&sealed, &sets, out, Some(&candidate))?;
    history.advance(&sealed, &path)?;
    Ok(Verdict::Pass)
}

/// A reviewed amendment payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendmentReview {
    /// [`FORMAT_VERSION`].
    pub format_version: u32,
    /// The plan's reviewer.
    pub reviewer: String,
    /// The journaled batch the amendment backfills.
    pub batch_sha256: String,
    /// Newly measured checkpoint-2 losses among the batch's witnesses, the
    /// sole retrospective seed extension.
    pub seed_extension: Vec<LedgerRow>,
}

/// Backfills a journaled batch against main and every retained checkpoint,
/// observations replayed with each checkpoint's retained runner, recomputes
/// history over the expanded universe, and publishes an amendment receipt
/// when the reconstructed history stays monotone and fully adjudicated.
///
/// # Errors
///
/// [`DiffError`] when an input, identity or chain is invalid, a retained
/// capture is missing, or a comparison cannot run.
pub fn amend(
    history_dir: &Path,
    previous: &Path,
    witness_batch: &str,
    review: &Path,
    out: &Path,
    live: &LiveOptions,
) -> Result<Verdict, DiffError> {
    new_dir(out)?;
    if live.helmsweep.is_none() || live.selection.is_some() {
        return Err(DiffError::Request(
            "an amendment adjudicates every chart live; pass --helmsweep and no --chart".into(),
        ));
    }
    let Amendment {
        history,
        plan,
        plan_sha256,
        predecessor,
        review,
        universe_head,
        batch,
        captures,
        main,
    } = amendment(history_dir, previous, review)?;
    if review.batch_sha256 != witness_batch {
        return Err(DiffError::Request(format!(
            "--witness-batch {witness_batch} is not the reviewed batch {}",
            review.batch_sha256
        )));
    }
    let mut prior = PriorSets::default();
    let mut problems = Vec::new();
    for ordinal in 1..=predecessor.checkpoint_ordinal {
        let entry = captures
            .iter()
            .find(|entry| entry.ordinal == ordinal)
            .ok_or_else(|| DiffError::Identity(format!("checkpoint {ordinal} is not retained")))?;
        let table = backfill_run(
            &history,
            &main,
            entry,
            universe_head.as_deref(),
            live,
            out,
            &mut problems,
        )?;
        problems.extend(backfill_problems(&Backfill {
            ordinal,
            seed_ordinal: plan.seed_ordinal,
            table: &table,
            accepted: &history.prior_sets_at(ordinal)?,
            prior: &prior,
            batch: &batch,
            seed_extension: &review.seed_extension,
        }));
        prior = PriorSets {
            ordinal,
            regressions: table.regressions(),
            closures: table.closures(),
        };
    }
    if !problems.is_empty() {
        write_report(out, 1, &problems)?;
        return Ok(Verdict::Fail(problems));
    }
    let mut tables = Tables {
        hashes: BTreeMap::new(),
        payload_sha256: String::new(),
    };
    seal_payload(out, &mut tables)?;
    let seed_ledger_sha256 = if review.seed_extension.is_empty() {
        predecessor.seed_ledger_sha256.clone()
    } else {
        Some(canonical_sha256(&json!({
            "previous": predecessor.seed_ledger_sha256,
            "extension": review.seed_extension,
        }))?)
    };
    let sets = PriorSets {
        ordinal: predecessor.checkpoint_ordinal,
        ..prior
    };
    let universe = history.universe.load()?;
    if universe.head != universe_head {
        return Err(DiffError::Identity(
            "the universe moved during the amendment".into(),
        ));
    }
    let sealed = receipt(
        &ReceiptInputs {
            plan: &plan,
            plan_sha256: &plan_sha256,
            main: &main,
            candidate: None,
            predecessor: Some(&predecessor),
            universe: &universe,
            tables: &tables,
            seed_ledger_sha256,
        },
        ReceiptKind::Amendment,
        predecessor.checkpoint_ordinal,
        &sets,
    )?;
    sealed.write(&out.join("receipt.json"))?;
    let path = history.publish(&sealed, &sets, out, None)?;
    history.advance(&sealed, &path)?;
    Ok(Verdict::Pass)
}

/// An amendment's checked inputs.
struct Amendment {
    history: History,
    plan: HistoryPlan,
    plan_sha256: String,
    predecessor: Receipt,
    review: AmendmentReview,
    universe_head: Option<String>,
    batch: BTreeSet<String>,
    captures: Vec<RetainedCapture>,
    main: Capture,
}

/// Checks an amendment's review, its batch and the retained captures.
fn amendment(history_dir: &Path, previous: &Path, review: &Path) -> Result<Amendment, DiffError> {
    let history = History::new(history_dir);
    let (plan, plan_sha256) = history.plan()?;
    let predecessor = trusted_predecessor(&history, previous, &plan_sha256)?;
    let review: AmendmentReview = read_json(review)?;
    if review.format_version != FORMAT_VERSION || review.reviewer != plan.reviewer {
        return Err(DiffError::Identity(
            "the amendment is not reviewed by the plan's reviewer".into(),
        ));
    }
    let universe = history.universe.load()?;
    let covered = predecessor.witness_batch_hashes.len();
    let batches = universe.batch_hashes();
    if batches.get(..covered) != Some(&predecessor.witness_batch_hashes[..])
        || batches.get(covered) != Some(&review.batch_sha256)
        || batches.len() != covered + 1
    {
        return Err(DiffError::Identity(
            "the reviewed batch is not the one batch journaled after the predecessor".into(),
        ));
    }
    let batch = history.universe.batch_ids(&review.batch_sha256)?;
    let captures = retained_captures(&history)?;
    let main_entry = captures
        .iter()
        .find(|entry| entry.ordinal == 0)
        .ok_or_else(|| DiffError::Identity("main's capture is not retained".into()))?;
    let main = open_capture(&main_entry.path)?;
    if main.hash != main_entry.capture_hash {
        return Err(DiffError::Identity(
            "main's retained capture changed".into(),
        ));
    }
    Ok(Amendment {
        history,
        plan,
        plan_sha256,
        predecessor,
        review,
        universe_head: universe.head,
        batch,
        captures,
        main,
    })
}

/// Compares main with the retained checkpoint `entry` over the expanded
/// universe, writing its tables under `out`; failures and discoveries become
/// `problems`. Its artifact changes were certified when it was accepted.
fn backfill_run(
    history: &History,
    main: &Capture,
    entry: &RetainedCapture,
    universe_head: Option<&str>,
    live: &LiveOptions,
    out: &Path,
    problems: &mut Vec<String>,
) -> Result<OutcomeTable, DiffError> {
    // Each backfill consumes a fresh load, so no two universes are held.
    let universe = history.universe.load()?;
    if universe.head.as_deref() != universe_head {
        return Err(DiffError::Identity(
            "the universe moved during the amendment".into(),
        ));
    }
    let ordinal = entry.ordinal;
    let candidate = open_capture(&entry.path)?;
    if candidate.hash != entry.capture_hash {
        return Err(DiffError::Identity(format!(
            "checkpoint {ordinal} capture changed"
        )));
    }
    let options = LiveOptions {
        helmsweep: live.helmsweep.clone(),
        contexts: live.contexts.clone(),
        selection: None,
        scratch: live.scratch.join(format!("checkpoint-{ordinal:02}")),
    };
    let mut comparison = compare_captures(
        main,
        &candidate,
        universe,
        history.universe.blobs(),
        &options,
        &format!("attempt:backfill-{ordinal}"),
    )?;
    if !comparison.new_witnesses.is_empty() {
        problems.push(format!(
            "backfilling checkpoint {ordinal} discovered {} further witnesses",
            comparison.new_witnesses.len()
        ));
    }
    problems.extend(
        comparison
            .failures
            .iter()
            .map(|failure| format!("checkpoint {ordinal}: {failure}")),
    );
    let changes = changes_of(main, &candidate, &comparison, &[], None)?;
    let dir = out.join(format!("checkpoint-{ordinal:02}"));
    std::fs::create_dir_all(&dir).map_err(|error| io(&dir, &error))?;
    write_tables(&dir, &mut comparison, &changes)?;
    Ok(comparison.run.table)
}

/// One checkpoint's backfilled outcomes and what they must agree with.
struct Backfill<'a> {
    ordinal: u32,
    seed_ordinal: u32,
    table: &'a OutcomeTable,
    accepted: &'a PriorSets,
    prior: &'a PriorSets,
    batch: &'a BTreeSet<String>,
    seed_extension: &'a [LedgerRow],
}

/// Why a backfilled checkpoint breaks the reconstructed history: nothing
/// may be unadjudicated or infrastructure, checkpoint 1 must still equal
/// main, the seed may grow only by exactly the reviewed, newly journaled
/// witnesses, later checkpoints may only shrink, and no closure earned by the
/// accepted or the reconstructed predecessor may be lost.
fn backfill_problems(backfill: &Backfill<'_>) -> Vec<String> {
    let Backfill {
        ordinal,
        seed_ordinal,
        table,
        accepted,
        prior,
        batch,
        seed_extension,
    } = backfill;
    let blocking = table.blocking();
    let mut problems = blocking
        .semantic
        .into_iter()
        .chain(blocking.infrastructure)
        .map(|problem| format!("checkpoint {ordinal}: {problem}"))
        .collect::<Vec<_>>();
    let regressions = table.regressions();
    if *ordinal == 1 {
        problems.extend(
            table
                .inequalities()
                .into_iter()
                .map(|inequality| format!("checkpoint 1 no longer equals main: {inequality}")),
        );
    } else if ordinal == seed_ordinal {
        let exposed = regressions
            .difference(&accepted.regressions)
            .cloned()
            .collect::<BTreeSet<_>>();
        if let Some(known) = exposed.iter().find(|key| !batch.contains(&key.witness)) {
            problems.push(format!(
                "{} was known before the batch and cannot extend the seed",
                known.witness
            ));
        }
        let reviewed = keys(seed_extension);
        if exposed != reviewed {
            problems.push(format!(
                "checkpoint {ordinal} newly exposes {exposed:?}, reviewed {reviewed:?}"
            ));
        }
    } else {
        for added in regressions.difference(&prior.regressions) {
            problems.push(format!(
                "checkpoint {ordinal} regresses {} ({})",
                added.witness, added.direction
            ));
        }
    }
    let closures = table.closures();
    let earned = accepted
        .closures
        .union(&prior.closures)
        .collect::<BTreeSet<_>>();
    for lost in earned {
        if !closures.contains(lost) {
            problems.push(format!("checkpoint {ordinal} loses earned closure {lost}"));
        }
    }
    problems
}

/// One retained capture of the history.
#[derive(Clone, Debug, Deserialize)]
struct RetainedCapture {
    ordinal: u32,
    capture_hash: String,
    path: PathBuf,
}

fn retained_captures(history: &History) -> Result<Vec<RetainedCapture>, DiffError> {
    let path = history.root.join("captures.jsonl");
    let text = std::fs::read_to_string(&path).map_err(|error| io(&path, &error))?;
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))
        })
        .collect()
}

impl History {
    /// The sets of the last accepted receipt at `ordinal`: an amendment's
    /// when one replaced the checkpoint's.
    fn prior_sets_at(&self, ordinal: u32) -> Result<PriorSets, DiffError> {
        let dir = self.root.join("receipts");
        let entries = std::fs::read_dir(&dir).map_err(|error| io(&dir, &error))?;
        let prefix = format!("{ordinal:02}-Checkpoint-");
        for entry in entries {
            let entry = entry.map_err(|error| io(&dir, &error))?;
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let receipt = Receipt::read(&entry.path())?;
                return self.prior_sets(&receipt);
            }
        }
        Err(DiffError::Identity(format!(
            "no accepted receipt at {ordinal}"
        )))
    }
}

/// Verifies a capture.
///
/// # Errors
///
/// [`DiffError::Identity`] when it does not match its manifest.
pub fn verify_capture(root: &Path) -> Result<Verdict, DiffError> {
    open_capture(root).map(|_| Verdict::Pass)
}

/// Rehashes a receipt's payload: its index, every file it lists, and the
/// table hashes the receipt names.
fn verify_payload(history: &History, receipt: &Receipt) -> Result<(), DiffError> {
    let dir = history.payload_dir(&receipt.payload_index_sha256);
    let index = std::fs::read(dir.join("payload.json")).map_err(|error| {
        DiffError::Identity(format!("payload of {}: {error}", receipt.seal_sha256))
    })?;
    if sha256_hex(&index) != receipt.payload_index_sha256 {
        return Err(DiffError::Identity(format!(
            "the payload of {} changed",
            receipt.seal_sha256
        )));
    }
    let mut listed = BTreeMap::new();
    for PayloadFile { path, sha256 } in read_payload(&index)?.files {
        let actual = file_sha256(&dir.join(&path))
            .map_err(|error| DiffError::Identity(format!("payload file {path}: {error}")))?;
        if actual != sha256 {
            return Err(DiffError::Identity(format!("payload file {path} changed")));
        }
        listed.insert(path, actual);
    }
    let named = |prefix: &str, file: &str, hash: &str| -> Result<(), DiffError> {
        let path = if prefix.is_empty() {
            file.to_string()
        } else {
            format!("{prefix}/{file}")
        };
        if hash.is_empty() || listed.get(&path).map(String::as_str) == Some(hash) {
            Ok(())
        } else {
            Err(DiffError::Identity(format!(
                "receipt {} names a {path} its payload does not hold",
                receipt.seal_sha256
            )))
        }
    };
    if receipt.kind != ReceiptKind::Amendment {
        named("", "outcomes.jsonl", &receipt.full_outcome_table_hash)?;
        named("", "coverage.json", &receipt.coverage_hash)?;
        named("", "changes.jsonl", &receipt.proof_index_hash)?;
        if let Some(hash) = &receipt.observation_results_hash {
            named("", "observations.jsonl", hash)?;
        }
    }
    Ok(())
}

/// Refuses `current` unless it extends `previous` in ordinal, baseline,
/// journal, universe, experiment and tool.
fn follows(current: &Receipt, previous: &Receipt) -> Result<(), DiffError> {
    let expected_ordinal = match current.kind {
        ReceiptKind::Amendment | ReceiptKind::Migration => previous.checkpoint_ordinal,
        ReceiptKind::Checkpoint | ReceiptKind::Landing => previous.checkpoint_ordinal + 1,
        ReceiptKind::Genesis => {
            return Err(DiffError::Identity(
                "genesis cannot have a predecessor".into(),
            ));
        }
    };
    if current.checkpoint_ordinal != expected_ordinal
        || current.baseline_capture_hash != previous.baseline_capture_hash
        || current.witness_batch_hashes.len() < previous.witness_batch_hashes.len()
        || current.universe_parent_hash.as_deref() != Some(previous.universe_hash.as_str())
        || Experiment::recorded(current) != Experiment::recorded(previous)
        || current.tool_version_and_hash != previous.tool_version_and_hash
    {
        return Err(DiffError::Identity(format!(
            "receipt {} does not follow {}",
            current.seal_sha256, previous.seal_sha256
        )));
    }
    Ok(())
}

/// Verifies a receipt against its history: its seal, its predecessor chain
/// back to genesis, its sets, its payload, its plan, its experiment, the
/// universe journal it names and every capture and retained runner it
/// references.
///
/// # Errors
///
/// [`DiffError::Identity`] on any broken link.
pub fn verify_history(history_dir: &Path, receipt_path: &Path) -> Result<Verdict, DiffError> {
    let history = History::new(history_dir);
    let (plan, plan_sha256) = history.plan()?;
    let universe = history.universe.load()?;
    let mut receipts = BTreeMap::new();
    let dir = history.root.join("receipts");
    for entry in std::fs::read_dir(&dir).map_err(|error| io(&dir, &error))? {
        let entry = entry.map_err(|error| io(&dir, &error))?;
        let receipt = Receipt::read(&entry.path())?;
        receipts.insert(receipt.seal_sha256.clone(), receipt);
    }
    let captures = retained_captures(&history)?
        .into_iter()
        .map(|entry| (entry.capture_hash.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let check_capture = |hash: &str| -> Result<(), DiffError> {
        let entry = captures
            .get(hash)
            .ok_or_else(|| DiffError::Identity(format!("capture {hash} is not retained")))?;
        if open_capture(&entry.path)?.hash != hash {
            return Err(DiffError::Identity(format!("capture {hash} changed")));
        }
        Ok(())
    };
    let mut current = Receipt::read(receipt_path)?;
    loop {
        if current.history_id != plan.history_id || current.plan_sha256 != plan_sha256 {
            return Err(DiffError::Identity(
                "a receipt belongs to another history or plan".into(),
            ));
        }
        history.prior_sets(&current)?;
        verify_payload(&history, &current)?;
        check_capture(&current.baseline_capture_hash)?;
        if let Some(capture) = &current.capture_hash {
            check_capture(capture)?;
        }
        let batches = universe.batch_hashes();
        if batches.get(..current.witness_batch_hashes.len())
            != Some(&current.witness_batch_hashes[..])
        {
            return Err(DiffError::Identity(format!(
                "receipt {} names batches the journal does not hold in order",
                current.seal_sha256
            )));
        }
        let Some(predecessor) = current.predecessor_receipt_sha256.clone() else {
            if current.kind != ReceiptKind::Genesis {
                return Err(DiffError::Identity(
                    "only genesis may lack a predecessor".into(),
                ));
            }
            return Ok(Verdict::Pass);
        };
        let previous = receipts
            .get(&predecessor)
            .ok_or_else(|| DiffError::Identity(format!("predecessor {predecessor} is missing")))?
            .clone();
        follows(&current, &previous)?;
        current = previous;
    }
}

/// The admission outcomes of a comparison output directory.
///
/// # Errors
///
/// [`DiffError::Identity`] when `admissions.jsonl` is missing or malformed.
pub fn read_admissions(out: &Path) -> Result<Vec<AdmissionOutcome>, DiffError> {
    read_table(&out.join("admissions.jsonl"))
}

fn read_table<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, DiffError> {
    let text = std::fs::read_to_string(path).map_err(|error| io(path, &error))?;
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))
        })
        .collect()
}

/// The outcome table of a comparison output directory.
///
/// # Errors
///
/// [`DiffError::Identity`] when a table is missing or malformed.
pub fn read_outcomes(out: &Path) -> Result<OutcomeTable, DiffError> {
    let mut table = OutcomeTable::new(read_table(&out.join("outcomes.jsonl"))?);
    table.observations = read_table(&out.join("observations.jsonl"))?;
    table.admissions = read_table(&out.join("admissions.jsonl"))?;
    Ok(table)
}

/// The regression keys of `table`, for writing a ledger.
#[must_use]
pub fn regression_keys(table: &OutcomeTable) -> BTreeSet<RegressionKey> {
    table.regressions()
}
