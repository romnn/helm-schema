//! The differential gate's commands: capture, self-test, init, compare,
//! amend and verify. Every path is explicit, every output directory new,
//! and every policy decision is made here, in typed code.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::capture::{DumpPair, VerifiedDump};
use super::compare::{
    Class, ConstructionRequest, LedgerRow, OutcomeTable, PriorSets, RegressionKey, SeedReview,
    construction_policy,
};
use super::controls::{
    ControlRegistry, ControlResponseV1, DescribeResponse, ObservationOutcome, ObservationRecord,
    Runner, classify_observation, copy_registry, observe_case,
};
use super::history::{
    Attempt, FORMAT_VERSION, GateHead, HistoryPlan, Receipt, ReceiptKind, Universe, UniverseState,
    canonical_json, canonical_sha256, write_new,
};
use super::{
    ArtifactChange, ComparisonRun, DiffError, EquivalenceCertificate, RunOptions, artifact_changes,
    run_comparison,
};
use crate::acceptance::pool::PoolLimits;
use crate::acceptance::screen::ImportedRoster;
use crate::helm::adjudication::{KUBERNETES_RELEASE, OfflineKubernetesValidator};
use crate::helm::invocation::{HelmRunner, RENDER_TIMEOUT};
use crate::manifest::BuildProvenance;
use crate::source_digest::{generation_source_digest, sha256_hex};

/// A command that ran to a verdict.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Complete and policy-satisfied: exit code 0.
    Pass,
    /// A semantic, unadjudicated or monotonicity failure: exit code 1.
    Fail(Vec<String>),
}

/// The capture manifest, format version 1, scope `schema-dumps-v1`: one
/// revision's producer dump with its source, build and Helm provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureManifest {
    /// [`FORMAT_VERSION`].
    pub format_version: u32,
    /// What the capture holds; `schema-dumps-v1` has no admission, export or
    /// control-runner jobs yet.
    pub scope: String,
    /// The checkout's commit.
    pub commit: String,
    /// The commit's tree.
    pub tree: String,
    /// The checkout's absolute path.
    pub checkout: String,
    /// `git status --porcelain` of the checkout; empty unless synthetic.
    pub status: String,
    /// Whether the capture was taken from a dirty tree for a synthetic
    /// demonstration; synthetic captures never enter a construction history.
    pub synthetic: bool,
    /// The checkout's generation source digest when captured.
    pub source_sha256: String,
    /// SHA-256 of the copied producer executable.
    pub producer_sha256: String,
    /// The build the producer was compiled as.
    pub producer_build: BuildProvenance,
    /// SHA-256 of the dump's manifest.
    pub dump_manifest_sha256: String,
    /// SHA-256 of the helmsweep executable.
    pub helmsweep_sha256: String,
    /// helmsweep's version report.
    pub helmsweep_version: String,
    /// The pinned Kubernetes release.
    pub kubernetes_release: String,
    /// SHA-256 of the dump's registry entries and recipe inputs.
    pub suite_sha256: String,
    /// SHA-256 of the context roster's exact bytes, when one was captured.
    pub contexts_sha256: Option<String>,
    /// The observation controls, when captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<ControlsCapture>,
}

/// What a capture's `controls/` directory must hash to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlsCapture {
    /// SHA-256 of the copied registry's manifest.
    pub registry: String,
    /// SHA-256 of the copied runner executable: its build id.
    pub runner: String,
    /// SHA-256 of the runner's describe response.
    pub describe: String,
    /// SHA-256 of `observations.jsonl`.
    pub observations: String,
}

/// A capture's verified controls: the registry copy, the retained runner
/// and one validated observation per scored case.
#[derive(Debug)]
pub struct CapturedControls {
    /// The registry copy.
    pub registry: ControlRegistry,
    /// The retained runner.
    pub runner: Runner,
    /// The runner's self-description.
    pub describe: DescribeResponse,
    /// The observations by case id.
    pub observations: BTreeMap<String, ObservationRecord>,
}

/// An opened, verified capture.
#[derive(Debug)]
pub struct Capture {
    /// The capture directory.
    pub root: PathBuf,
    /// Its manifest.
    pub manifest: CaptureManifest,
    /// SHA-256 of its manifest bytes.
    pub hash: String,
    /// Its verified dump.
    pub dump: VerifiedDump,
    /// Its verified controls, when captured.
    pub controls: Option<CapturedControls>,
}

/// The control registry and the same-revision runner a capture evaluates.
pub struct ControlsRequest {
    /// The registry directory.
    pub registry: PathBuf,
    /// The revision's IR library test executable.
    pub runner: PathBuf,
}

/// What `capture` records.
pub struct CaptureRequest {
    /// The revision's `corpus_generation` executable.
    pub producer: PathBuf,
    /// The revision's checkout, which the producer was built from.
    pub checkout: PathBuf,
    /// The commit the checkout must be at.
    pub commit: String,
    /// The pinned helmsweep executable.
    pub helmsweep: PathBuf,
    /// The context roster to retain, if any.
    pub contexts: Option<PathBuf>,
    /// The new capture directory.
    pub out: PathBuf,
    /// Allows a dirty tree, marking the capture synthetic.
    pub synthetic: bool,
    /// The observation controls to evaluate, if any.
    pub controls: Option<ControlsRequest>,
}

fn io(path: &Path, error: &std::io::Error) -> DiffError {
    DiffError::Infrastructure(format!("{}: {error}", path.display()))
}

fn run(program: &Path, args: &[&str], cwd: &Path, log: &Path) -> Result<String, DiffError> {
    let output = std::process::Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| io(program, &error))?;
    let mut record = format!(
        "$ {} {}\nstatus: {}\n",
        program.display(),
        args.join(" "),
        output.status
    )
    .into_bytes();
    record.extend_from_slice(b"--- stdout\n");
    record.extend_from_slice(&output.stdout);
    record.extend_from_slice(b"--- stderr\n");
    record.extend_from_slice(&output.stderr);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|error| io(log, &error))?;
    file.write_all(&record).map_err(|error| io(log, &error))?;
    if !output.status.success() {
        return Err(DiffError::Infrastructure(format!(
            "{} {} failed with {}; see {}",
            program.display(),
            args.join(" "),
            output.status,
            log.display()
        )));
    }
    String::from_utf8(output.stdout)
        .map(|stdout| stdout.trim().to_string())
        .map_err(|error| DiffError::Infrastructure(format!("{}: {error}", program.display())))
}

fn file_sha256(path: &Path) -> Result<String, DiffError> {
    std::fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| io(path, &error))
}

/// Captures one revision: its producer's complete dump, verified by that
/// same producer, with its source, build and Helm provenance.
///
/// # Errors
///
/// [`DiffError::Identity`] when the checkout is not at `commit`, is dirty
/// without `synthetic`, or the producer was built from another tree;
/// [`DiffError::Infrastructure`] when a process or file fails.
pub fn capture(request: &CaptureRequest) -> Result<Capture, DiffError> {
    let CaptureRequest {
        producer,
        checkout,
        commit,
        helmsweep,
        contexts,
        out,
        synthetic,
        controls,
    } = request;
    if out.exists() {
        return Err(DiffError::Request(format!(
            "{} already exists",
            out.display()
        )));
    }
    let logs = out.join("logs");
    std::fs::create_dir_all(&logs).map_err(|error| io(&logs, &error))?;
    let log = logs.join("capture.log");
    let git = Path::new("git");
    let head = run(git, &["rev-parse", "HEAD"], checkout, &log)?;
    if head != *commit {
        return Err(DiffError::Identity(format!(
            "{} is at {head}, not {commit}",
            checkout.display()
        )));
    }
    let tree = run(git, &["rev-parse", "HEAD^{tree}"], checkout, &log)?;
    let status = run(git, &["status", "--porcelain"], checkout, &log)?;
    if !status.is_empty() && !synthetic {
        return Err(DiffError::Identity(format!(
            "{} has uncommitted changes:\n{status}",
            checkout.display()
        )));
    }
    let source_sha256 = generation_source_digest(checkout).map_err(|error| io(checkout, &error))?;
    let bin = out.join("bin");
    std::fs::create_dir_all(&bin).map_err(|error| io(&bin, &error))?;
    let copied = bin.join("corpus_generation");
    std::fs::copy(producer, &copied).map_err(|error| io(producer, &error))?;
    let dump_dir = out.join("dump");
    let dump_arg = dump_dir.to_string_lossy().into_owned();
    run(
        &copied,
        &["--out", &dump_arg, "--helm-ready"],
        checkout,
        &log,
    )?;
    run(&copied, &["--verify", &dump_arg], checkout, &log)?;
    let dump = VerifiedDump::open(&dump_dir)?;
    if dump.build().source_sha256 != source_sha256 {
        return Err(DiffError::Identity(format!(
            "the producer was built from source {}, but {} hashes to {source_sha256}",
            dump.build().source_sha256,
            checkout.display()
        )));
    }
    let helmsweep_version = run(helmsweep, &["version"], checkout, &log)?;
    let contexts_sha256 = match contexts {
        Some(path) => {
            let target = out.join("contexts.json");
            std::fs::copy(path, &target).map_err(|error| io(path, &error))?;
            Some(file_sha256(&target)?)
        }
        None => None,
    };
    let controls = controls
        .as_ref()
        .map(|controls| record_controls(out, controls, dump.build()))
        .transpose()?;
    let manifest = CaptureManifest {
        format_version: FORMAT_VERSION,
        scope: "schema-dumps-v1".into(),
        commit: commit.clone(),
        tree,
        checkout: checkout.to_string_lossy().into_owned(),
        status,
        synthetic: *synthetic,
        source_sha256,
        producer_sha256: file_sha256(&copied)?,
        producer_build: dump.build().clone(),
        dump_manifest_sha256: dump.manifest_sha256().to_string(),
        helmsweep_sha256: file_sha256(helmsweep)?,
        helmsweep_version,
        kubernetes_release: KUBERNETES_RELEASE.into(),
        suite_sha256: suite_sha256(&dump)?,
        contexts_sha256,
        controls,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    write_new(&out.join("capture.json"), &bytes)?;
    open_capture(out)
}

/// SHA-256 of a dump's keys, recipes and recipe inputs: what both sides of
/// a comparison must share.
///
/// # Errors
///
/// [`DiffError`] when an entry cannot be read or serialized.
pub fn suite_sha256(dump: &VerifiedDump) -> Result<String, DiffError> {
    let mut suite = Vec::new();
    for key in dump.keys() {
        let schema = dump.entry(key)?;
        suite.push(json!([key, schema.recipe, schema.inputs_sha256]));
    }
    canonical_sha256(&suite)
}

/// Opens and verifies the capture at `root`.
///
/// # Errors
///
/// [`DiffError::Identity`] when the manifest, the dump or the producer copy
/// disagree with what the capture recorded.
pub fn open_capture(root: &Path) -> Result<Capture, DiffError> {
    let path = root.join("capture.json");
    let bytes = std::fs::read(&path).map_err(|error| io(&path, &error))?;
    let manifest: CaptureManifest = serde_json::from_slice(&bytes)
        .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))?;
    if manifest.format_version != FORMAT_VERSION || manifest.scope != "schema-dumps-v1" {
        return Err(DiffError::Identity(format!(
            "{} is not a version-1 schema-dumps capture",
            path.display()
        )));
    }
    let dump = VerifiedDump::open(&root.join("dump"))?;
    if dump.manifest_sha256() != manifest.dump_manifest_sha256
        || *dump.build() != manifest.producer_build
        || file_sha256(&root.join("bin/corpus_generation"))? != manifest.producer_sha256
        || suite_sha256(&dump)? != manifest.suite_sha256
    {
        return Err(DiffError::Identity(format!(
            "{} no longer matches its capture manifest",
            root.display()
        )));
    }
    let controls = manifest
        .controls
        .as_ref()
        .map(|controls| open_controls(&root.join("controls"), controls))
        .transpose()?;
    Ok(Capture {
        root: root.to_path_buf(),
        hash: sha256_hex(&bytes),
        manifest,
        dump,
        controls,
    })
}

/// Copies the registry and runner into `out/controls`, checks the runner was
/// built from the producer's source, and records one observation per
/// scored case.
///
/// # Errors
///
/// [`DiffError::Identity`] when the runner was built from another source or
/// answers another request, input, query or build;
/// [`DiffError::Infrastructure`] when a runner invocation fails.
pub fn record_controls(
    out: &Path,
    request: &ControlsRequest,
    build: &BuildProvenance,
) -> Result<ControlsCapture, DiffError> {
    let dir = out.join("controls");
    let registry = copy_registry(
        &ControlRegistry::open(&request.registry)?,
        &dir.join("registry"),
    )?;
    let runner_dir = dir.join("runner");
    std::fs::create_dir_all(&runner_dir).map_err(|error| io(&runner_dir, &error))?;
    let copied = runner_dir.join("protocol-runner");
    std::fs::copy(&request.runner, &copied).map_err(|error| io(&request.runner, &error))?;
    let runner = Runner::new(&copied)?;
    let describe = runner.describe(&dir.join("runs/describe"))?;
    if describe.build != *build {
        return Err(DiffError::Identity(format!(
            "the control runner was built from {:?}, the producer from {build:?}",
            describe.build
        )));
    }
    let describe_bytes = json_line(&describe)?;
    write_new(&dir.join("describe.json"), describe_bytes.as_bytes())?;
    let mut lines = Vec::new();
    for case in registry.cases.values() {
        if case.scored_query().is_none() {
            continue;
        }
        let record = observe_case(
            &runner,
            &dir.join("runs").join(&case.id),
            &dir.join("registry/blobs"),
            case,
        )?;
        lines.push(json_line(&record)?);
    }
    Ok(ControlsCapture {
        registry: registry.sha256,
        runner: runner.build_id,
        describe: sha256_hex(describe_bytes.as_bytes()),
        observations: write_lines(&dir.join("observations.jsonl"), &lines)?,
    })
}

/// Opens a capture's controls and checks them against its manifest.
fn open_controls(dir: &Path, expected: &ControlsCapture) -> Result<CapturedControls, DiffError> {
    let registry = ControlRegistry::open(&dir.join("registry"))?;
    let runner = Runner::new(&dir.join("runner/protocol-runner"))
        .map_err(|error| DiffError::Identity(format!("retained runner: {error}")))?;
    let describe_bytes =
        std::fs::read(dir.join("describe.json")).map_err(|error| io(dir, &error))?;
    let observations_bytes =
        std::fs::read(dir.join("observations.jsonl")).map_err(|error| io(dir, &error))?;
    if registry.sha256 != expected.registry
        || runner.build_id != expected.runner
        || sha256_hex(&describe_bytes) != expected.describe
        || sha256_hex(&observations_bytes) != expected.observations
    {
        return Err(DiffError::Identity(format!(
            "{} no longer matches its capture manifest",
            dir.display()
        )));
    }
    let describe: DescribeResponse = serde_json::from_slice(&describe_bytes)
        .map_err(|error| DiffError::Identity(format!("describe.json: {error}")))?;
    let mut observations = BTreeMap::new();
    for line in observations_bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let record: ObservationRecord = serde_json::from_slice(line)
            .map_err(|error| DiffError::Identity(format!("observations.jsonl: {error}")))?;
        if record.response.runner_build_id != runner.build_id {
            return Err(DiffError::Identity(format!(
                "{}: answered by another runner",
                record.case
            )));
        }
        observations.insert(record.case.clone(), record);
    }
    Ok(CapturedControls {
        registry,
        runner,
        describe,
        observations,
    })
}

/// Re-evaluates `case`, which `registry` holds but the capture never saw,
/// with the capture's retained runner on a fresh blob root under the new
/// directory `scratch`: no checkout, current runner or Helm answer is used.
///
/// # Errors
///
/// [`DiffError::Identity`] when the capture has no retained runner or a
/// blob is missing; otherwise as [`observe_case`].
pub fn backfill_observation(
    capture: &Capture,
    registry: &ControlRegistry,
    case_id: &str,
    scratch: &Path,
) -> Result<ObservationRecord, DiffError> {
    let controls = capture.controls.as_ref().ok_or_else(|| {
        DiffError::Identity(format!(
            "{} retains no control runner",
            capture.root.display()
        ))
    })?;
    let runner = Runner::new(&controls.runner.path)
        .map_err(|error| DiffError::Identity(format!("retained runner: {error}")))?;
    if runner.build_id != controls.runner.build_id {
        return Err(DiffError::Identity("the retained runner changed".into()));
    }
    let case = registry
        .cases
        .get(case_id)
        .ok_or_else(|| DiffError::Request(format!("{case_id} is not in the registry")))?;
    if scratch.exists() {
        return Err(DiffError::Identity(format!(
            "{} already exists",
            scratch.display()
        )));
    }
    let blob_root = scratch.join("blobs");
    super::controls::stage_case_blobs(registry, case, &blob_root)?;
    observe_case(&runner, &scratch.join("run"), &blob_root, case)
}

/// Classifies every scored control both captures answered.
///
/// # Errors
///
/// [`DiffError::Identity`] when only one capture has controls, the
/// registries differ or a scored case lacks an answer;
/// [`DiffError::Infrastructure`] for an infrastructure answer.
pub fn observation_outcomes(
    main: &Capture,
    candidate: &Capture,
) -> Result<Vec<ObservationOutcome>, DiffError> {
    let (main, candidate) = match (&main.controls, &candidate.controls) {
        (None, None) => return Ok(Vec::new()),
        (Some(main), Some(candidate)) => (main, candidate),
        (Some(_), None) | (None, Some(_)) => {
            return Err(DiffError::Identity(
                "only one capture has observation controls".into(),
            ));
        }
    };
    if main.registry.sha256 != candidate.registry.sha256 {
        return Err(DiffError::Identity(
            "the captures evaluated different control registries".into(),
        ));
    }
    let answer =
        |controls: &CapturedControls, case: &str| -> Result<ControlResponseV1, DiffError> {
            controls
                .observations
                .get(case)
                .map(|record| record.response.clone())
                .ok_or_else(|| DiffError::Identity(format!("{case}: no captured observation")))
        };
    let mut outcomes = Vec::new();
    for case in main.registry.cases.values() {
        if case.scored_query().is_none() {
            continue;
        }
        outcomes.push(classify_observation(
            case,
            &answer(main, &case.id)?.result,
            &answer(candidate, &case.id)?.result,
        )?);
    }
    Ok(outcomes)
}

/// How live adjudication runs, shared by every comparing command.
pub struct LiveOptions {
    /// The pinned helmsweep; `None` judges with the schemas alone.
    pub helmsweep: Option<PathBuf>,
    /// The context roster file, if any.
    pub contexts: Option<PathBuf>,
    /// The charts to compare; a selection is diagnostic only.
    pub selection: Option<BTreeSet<String>>,
    /// Scratch for the Helm runner.
    pub scratch: PathBuf,
}

/// The Helm runner, validator and roster of one comparison.
struct LiveInputs {
    helm: Option<&'static HelmRunner>,
    kubernetes: Option<OfflineKubernetesValidator>,
    contexts: Option<ImportedRoster>,
}

/// The Helm runner, validator and roster for `options`.
fn live_inputs(options: &LiveOptions, charts: &BTreeSet<String>) -> Result<LiveInputs, DiffError> {
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
    let runner = HelmRunner::with_helmsweep(
        &options.scratch.join("helm"),
        false,
        helmsweep.clone(),
        RENDER_TIMEOUT,
    )
    .map_err(|error| DiffError::Identity(format!("helmsweep: {error:#}")))?;
    let testdata = test_util::workspace_testdata();
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

/// Compares two captures over `universe` and the run's fresh discovery.
fn compare_captures(
    main: &Capture,
    candidate: &Capture,
    universe: &UniverseState,
    blobs: &super::history::BlobStore,
    options: &LiveOptions,
    origin: &str,
) -> Result<(DumpPair, ComparisonRun), DiffError> {
    if main.manifest.suite_sha256 != candidate.manifest.suite_sha256 {
        return Err(DiffError::Identity(
            "the captures screen different suites".to_string(),
        ));
    }
    let pair = DumpPair::new(
        VerifiedDump::open(main.dump.root())?,
        VerifiedDump::open(candidate.dump.root())?,
        &test_util::workspace_testdata(),
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
    } = live_inputs(options, &labels)?;
    let observations = observation_outcomes(main, candidate)?;
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
    if let (Some(coverage), Some(controls)) = (run.coverage.as_object_mut(), &main.controls) {
        coverage.insert(
            "observations".into(),
            super::controls::coverage(&observations, &controls.registry),
        );
    }
    run.table.observations = observations;
    run.table
        .observations
        .sort_by(|left, right| left.witness.cmp(&right.witness));
    Ok((pair, run))
}

/// The files every comparison writes, and their hashes.
struct Tables {
    hashes: BTreeMap<&'static str, String>,
}

fn write_lines(path: &Path, lines: &[String]) -> Result<String, DiffError> {
    let mut bytes = Vec::new();
    for line in lines {
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
    }
    write_new(path, &bytes)?;
    Ok(sha256_hex(&bytes))
}

fn json_line<T: Serialize>(value: &T) -> Result<String, DiffError> {
    let value = serde_json::to_value(value)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    String::from_utf8(canonical_json(&value))
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))
}

/// Writes the outcome tables of `run` into the new directory `out`.
fn write_tables(
    out: &Path,
    run: &ComparisonRun,
    changes: &[ArtifactChange],
) -> Result<Tables, DiffError> {
    let mut hashes = BTreeMap::new();
    let mut cells = vec![
        "witness\tcase\tclass\tbaseline_accepts\tcandidate_accepts\toracle\tdirection".to_string(),
    ];
    for outcome in &run.table.outcomes {
        cells.push(format!(
            "{}\t{}\t{:?}\t{}\t{}\t{:?}\t{}",
            outcome.witness,
            outcome.case,
            outcome.class,
            outcome.baseline_accepts,
            outcome.candidate_accepts,
            outcome.oracle,
            outcome.direction.as_deref().unwrap_or("")
        ));
    }
    hashes.insert("cells", write_lines(&out.join("cells.tsv"), &cells)?);
    let outcomes = run
        .table
        .outcomes
        .iter()
        .map(json_line)
        .collect::<Result<Vec<_>, _>>()?;
    hashes.insert(
        "outcomes",
        write_lines(&out.join("outcomes.jsonl"), &outcomes)?,
    );
    let mut artifacts = vec!["key\tkind\tbaseline_sha256\tcandidate_sha256\tidentical".to_string()];
    for artifact in &run.artifacts {
        artifacts.push(format!(
            "{}\t{}\t{}\t{}\t{}",
            artifact.key,
            artifact.kind,
            artifact.baseline_sha256,
            artifact.candidate_sha256,
            artifact.baseline_sha256 == artifact.candidate_sha256
        ));
    }
    hashes.insert(
        "artifacts",
        write_lines(&out.join("artifacts.tsv"), &artifacts)?,
    );
    let changes = changes
        .iter()
        .map(json_line)
        .collect::<Result<Vec<_>, _>>()?;
    hashes.insert(
        "changes",
        write_lines(&out.join("changes.jsonl"), &changes)?,
    );
    let regressions = run
        .table
        .regressions()
        .iter()
        .map(|key| format!("{}\t{}", key.witness, key.direction))
        .collect::<Vec<_>>();
    hashes.insert(
        "regressions",
        write_lines(&out.join("regressions.tsv"), &regressions)?,
    );
    let closures = run.table.closures().into_iter().collect::<Vec<_>>();
    hashes.insert(
        "closures",
        write_lines(&out.join("closures.tsv"), &closures)?,
    );
    let witnesses = run
        .witnesses
        .values()
        .map(json_line)
        .collect::<Result<Vec<_>, _>>()?;
    hashes.insert(
        "witnesses",
        write_lines(&out.join("witnesses.jsonl"), &witnesses)?,
    );
    let observations = run
        .table
        .observations
        .iter()
        .map(json_line)
        .collect::<Result<Vec<_>, _>>()?;
    hashes.insert(
        "observations",
        write_lines(&out.join("observations.jsonl"), &observations)?,
    );
    let mut coverage = run.coverage.clone();
    if let Some(members) = coverage.as_object_mut() {
        members.insert("artifact_changes".into(), json!(changes.len()));
    }
    let coverage = serde_json::to_vec_pretty(&coverage)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    write_new(&out.join("coverage.json"), &coverage)?;
    hashes.insert("coverage", sha256_hex(&coverage));
    Ok(Tables { hashes })
}

/// Problems with changed artifacts no witness or certificate accounts for.
fn unadjudicated_changes(changes: &[ArtifactChange]) -> Vec<String> {
    changes
        .iter()
        .filter(|change| !change.adjudicated)
        .map(|change| {
            format!(
                "UNADJUDICATED artifact change {} at {:?}",
                change.artifact.key,
                change.pointers.iter().take(8).collect::<Vec<_>>()
            )
        })
        .collect()
}

/// Compares two captures without any history: the main==main self-test
/// and diagnostic comparisons. It never writes a receipt.
///
/// # Errors
///
/// [`DiffError`] when an input is invalid or the comparison cannot run.
pub fn self_test(
    main: &Path,
    candidate: &Path,
    out: &Path,
    options: &LiveOptions,
    certificates: &[EquivalenceCertificate],
) -> Result<Verdict, DiffError> {
    if out.exists() {
        return Err(DiffError::Request(format!(
            "{} already exists",
            out.display()
        )));
    }
    let main = open_capture(main)?;
    let candidate = open_capture(candidate)?;
    let scratch = out.join("scratch");
    let blobs = super::history::BlobStore::new(scratch.join("blobs"));
    let (pair, run) = compare_captures(
        &main,
        &candidate,
        &UniverseState::default(),
        &blobs,
        options,
        "attempt:self-test",
    )?;
    let changes = artifact_changes(&pair, &run, certificates)?;
    write_tables(out, &run, &changes)?;
    let counts = run.table.class_counts();
    let mut problems = Vec::new();
    for (class, count) in &counts {
        if !matches!(class, Class::Identical | Class::Inherited) {
            problems.push(format!("{count} witnesses are {class:?}"));
        }
    }
    let changed_artifacts = changes.len();
    if changed_artifacts > 0 {
        problems.push(format!("{changed_artifacts} artifacts changed bytes"));
    }
    if run.witnesses.is_empty() {
        problems.push("vacuous: no witness was judged".to_string());
    }
    let report = json!({
        "mode": "self-test",
        "main": main.hash, "candidate": candidate.hash,
        "witnesses": run.witnesses.len(),
        "classes": counts.iter().map(|(class, count)| (format!("{class:?}"), count)).collect::<BTreeMap<_, _>>(),
        "artifact_changes": changed_artifacts,
        "problems": problems,
    });
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    write_new(&out.join("report.json"), &bytes)?;
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

    fn plan(&self) -> Result<HistoryPlan, DiffError> {
        HistoryPlan::read(&self.root.join("plan.json"))
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

    /// Publishes `receipt`, its sets and its capture under the history.
    fn publish(
        &self,
        receipt: &Receipt,
        sets: &PriorSets,
        capture: Option<&Capture>,
    ) -> Result<PathBuf, DiffError> {
        let tables = self.tables_dir(&receipt.seal_sha256);
        let bytes = serde_json::to_vec_pretty(sets)
            .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
        write_new(&tables.join("sets.json"), &bytes)?;
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
    main: &'a Capture,
    candidate: Option<&'a Capture>,
    predecessor: Option<&'a Receipt>,
    universe: &'a UniverseState,
    tables: &'a Tables,
    seed_ledger_sha256: Option<String>,
    contexts: Option<String>,
}

fn receipt(
    inputs: &ReceiptInputs<'_>,
    kind: ReceiptKind,
    ordinal: u32,
    sets: &PriorSets,
    exit_code: u8,
) -> Result<Receipt, DiffError> {
    let ReceiptInputs {
        plan,
        main,
        candidate,
        predecessor,
        universe,
        tables,
        seed_ledger_sha256,
        contexts,
    } = inputs;
    let hash = |name: &str| tables.hashes.get(name).cloned().unwrap_or_default();
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
        suite_hash: main.manifest.suite_sha256.clone(),
        controls_hash: main
            .manifest
            .controls
            .as_ref()
            .map(|controls| controls.registry.clone()),
        context_roster_hash: contexts.clone(),
        provider_hash: main.manifest.suite_sha256.clone(),
        helm_identity: format!(
            "{} {}",
            main.manifest.helmsweep_sha256, main.manifest.kubernetes_release
        ),
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
        control_runner_manifest_hash: control_runner_manifest_hash(main, *candidate)?,
        observation_results_hash: main
            .manifest
            .controls
            .as_ref()
            .map(|_| hash("observations")),
        gate_mode: "construction".into(),
        landability: "NON-LANDABLE".into(),
        exit_code,
        seal_sha256: String::new(),
    }
    .seal()
}

/// SHA-256 of both captures' runner identities, when controls were captured.
fn control_runner_manifest_hash(
    main: &Capture,
    candidate: Option<&Capture>,
) -> Result<Option<String>, DiffError> {
    let Some(main_controls) = &main.manifest.controls else {
        return Ok(None);
    };
    let candidate_controls = candidate.and_then(|capture| capture.manifest.controls.as_ref());
    canonical_sha256(&json!({
        "main": [main_controls.runner, main_controls.describe],
        "candidate": candidate_controls
            .map(|controls| [controls.runner.clone(), controls.describe.clone()]),
    }))
    .map(Some)
}

/// Creates a history: main compared with itself discovers the first
/// universe batch and seals the genesis receipt at ordinal 0.
///
/// # Errors
///
/// [`DiffError`] when an input is invalid, main is synthetic, or main does
/// not compare identically with itself.
pub fn init(
    main: &Path,
    plan: &Path,
    out: &Path,
    options: &LiveOptions,
) -> Result<Verdict, DiffError> {
    if out.exists() {
        return Err(DiffError::Request(format!(
            "{} already exists",
            out.display()
        )));
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
    let history = History::new(out);
    write_new(
        &out.join("plan.json"),
        &std::fs::read(plan).map_err(|error| io(plan, &error))?,
    )?;
    let lock = history.universe.lock()?;
    let mut universe = history.universe.load()?;
    let (pair, run) = compare_captures(
        &main,
        &main,
        &universe,
        history.universe.blobs(),
        options,
        "attempt:genesis",
    )?;
    let changes = artifact_changes(&pair, &run, &[])?;
    let not_identical = run
        .table
        .outcomes
        .iter()
        .filter(|outcome| outcome.class != Class::Identical)
        .count();
    if not_identical > 0 || !changes.is_empty() || run.witnesses.is_empty() {
        return Err(DiffError::Infrastructure(format!(
            "main does not compare identically with itself: {not_identical} outcomes, {} changes",
            changes.len()
        )));
    }
    history.universe.append(
        &lock,
        &mut universe,
        run.new_witnesses.clone(),
        Attempt {
            run_id: run_id(),
            capture_hash: Some(main.hash.clone()),
            reason: "genesis".into(),
        },
    )?;
    let tables = write_tables(&history.tables_dir("genesis"), &run, &changes)?;
    let sets = PriorSets::default();
    let genesis = receipt(
        &ReceiptInputs {
            plan: &plan_value,
            main: &main,
            candidate: None,
            predecessor: None,
            universe: &universe,
            tables: &tables,
            seed_ledger_sha256: None,
            contexts: main.manifest.contexts_sha256.clone(),
        },
        ReceiptKind::Genesis,
        0,
        &sets,
        0,
    )?;
    let path = history.publish(&genesis, &sets, Some(&main))?;
    let head = GateHead {
        format_version: FORMAT_VERSION,
        seal_sha256: genesis.seal_sha256.clone(),
        receipt: path
            .strip_prefix(out)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned(),
    };
    let bytes = serde_json::to_vec_pretty(&head)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    write_new(&out.join("gate-head.json"), &bytes)?;
    drop(lock);
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
    /// Reviewed equivalence certificates.
    pub certificates: Vec<EquivalenceCertificate>,
    /// Live adjudication.
    pub live: LiveOptions,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, DiffError> {
    let bytes = std::fs::read(path).map_err(|error| io(path, &error))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| DiffError::Request(format!("{}: {error}", path.display())))
}

/// The trusted predecessor: `previous` must be the history's gate head.
fn trusted_predecessor(history: &History, previous: &Path) -> Result<Receipt, DiffError> {
    let receipt = Receipt::read(previous)?;
    let head = history.gate_head()?;
    if head.seal_sha256 != receipt.seal_sha256 {
        return Err(DiffError::Identity(format!(
            "{} is not the trusted gate head {}",
            previous.display(),
            head.seal_sha256
        )));
    }
    Ok(receipt)
}

/// Everything a checkpoint comparison checked before comparing.
struct Checkpoint {
    history: History,
    plan: HistoryPlan,
    predecessor: Receipt,
    main: Capture,
    candidate: Capture,
    ordinal: u32,
    semantic: bool,
    prior: PriorSets,
    ledger: Vec<LedgerRow>,
    seed: Option<SeedReview>,
}

/// Checks a compare request's inputs and chain.
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
    if live.selection.is_some() {
        return Err(DiffError::Request(
            "a chart selection cannot certify a construction comparison".into(),
        ));
    }
    if live.helmsweep.is_none() {
        return Err(DiffError::Request(
            "a construction comparison adjudicates live; pass --helmsweep".into(),
        ));
    }
    let history = History::new(history);
    let plan = history.plan()?;
    let predecessor = trusted_predecessor(&history, previous)?;
    let main = open_capture(main)?;
    let candidate = open_capture(candidate)?;
    if predecessor.history_id != plan.history_id || predecessor.baseline_capture_hash != main.hash {
        return Err(DiffError::Identity(
            "the predecessor belongs to another history or main".into(),
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
/// journaled before any policy runs; a grown universe stops the comparison
/// until a reviewed amendment backfills history.
///
/// # Errors
///
/// [`DiffError`] when an input, identity or chain is invalid or the
/// comparison cannot run.
pub fn compare(request: &CompareRequest) -> Result<Verdict, DiffError> {
    let Checkpoint {
        history,
        plan,
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
    let lock = history.universe.lock()?;
    let mut universe = history.universe.load()?;
    if universe.head != predecessor.universe_journal_head_sha256 {
        return Err(DiffError::Identity(
            "the universe moved past the trusted predecessor; amend first".into(),
        ));
    }
    let (pair, run) = compare_captures(
        &main,
        &candidate,
        &universe,
        history.universe.blobs(),
        &request.live,
        &format!("attempt:checkpoint-{ordinal}"),
    )?;
    let changes = artifact_changes(&pair, &run, &request.certificates)?;
    let grown = history.universe.append(
        &lock,
        &mut universe,
        run.new_witnesses.clone(),
        Attempt {
            run_id: run_id(),
            capture_hash: Some(candidate.hash.clone()),
            reason: "discovery".into(),
        },
    )?;
    drop(lock);
    let tables = write_tables(out, &run, &changes)?;
    let mut problems = Vec::new();
    if let Some(batch) = &grown {
        problems.push(format!(
            "the universe grew by {} witnesses (batch {batch}); backfill history with amend",
            run.new_witnesses.len()
        ));
    }
    problems.extend(unadjudicated_changes(&changes));
    let policy = construction_policy(&ConstructionRequest {
        ordinal,
        semantic,
        table: &run.table,
        prior: &prior,
        ledger: &ledger,
        seed: seed.as_ref(),
        seed_ordinal: plan.seed_ordinal,
        reviewer: &plan.reviewer,
    })?;
    if !policy.infrastructure.is_empty() {
        write_report(out, 3, &policy.infrastructure)?;
        return Err(DiffError::Infrastructure(policy.infrastructure.join("; ")));
    }
    problems.extend(policy.semantic);
    if !problems.is_empty() {
        write_report(out, 1, &problems)?;
        return Ok(Verdict::Fail(problems));
    }
    let sets = PriorSets {
        ordinal,
        regressions: run.table.regressions(),
        closures: run.table.closures(),
    };
    let seed_ledger_sha256 = match &seed {
        Some(seed) => Some(canonical_sha256(seed)?),
        None => predecessor.seed_ledger_sha256.clone(),
    };
    let sealed = receipt(
        &ReceiptInputs {
            plan: &plan,
            main: &main,
            candidate: Some(&candidate),
            predecessor: Some(&predecessor),
            universe: &universe,
            tables: &tables,
            seed_ledger_sha256,
            contexts: candidate.manifest.contexts_sha256.clone(),
        },
        ReceiptKind::Checkpoint,
        ordinal,
        &sets,
        0,
    )?;
    sealed.write(&out.join("receipt.json"))?;
    history.publish(&sealed, &sets, Some(&candidate))?;
    Ok(Verdict::Pass)
}

fn write_report(out: &Path, exit_code: u8, problems: &[String]) -> Result<(), DiffError> {
    let report = json!({"exit_code": exit_code, "problems": problems, "receipt": null});
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    write_new(&out.join("report.json"), &bytes)
}

/// The regression keys of the reviewed rows.
fn row_keys(rows: &[LedgerRow]) -> BTreeSet<RegressionKey> {
    rows.iter()
        .map(|row| RegressionKey {
            witness: row.witness.clone(),
            direction: row.direction.clone(),
        })
        .collect()
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
/// recomputes history over the expanded universe, and publishes an
/// amendment receipt when the reconstructed history stays monotone.
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
    if out.exists() {
        return Err(DiffError::Request(format!(
            "{} already exists",
            out.display()
        )));
    }
    let Amendment {
        history,
        plan,
        predecessor,
        review,
        universe,
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
    let mut tables = BTreeMap::new();
    for ordinal in 1..=predecessor.checkpoint_ordinal {
        let entry = captures
            .iter()
            .find(|entry| entry.ordinal == ordinal)
            .ok_or_else(|| DiffError::Identity(format!("checkpoint {ordinal} is not retained")))?;
        let (run, outcomes) = backfill_run(&history, &main, entry, &universe, live, out)?;
        tables.insert(ordinal, outcomes);
        if !run.new_witnesses.is_empty() {
            problems.push(format!(
                "backfilling checkpoint {ordinal} discovered {} further witnesses",
                run.new_witnesses.len()
            ));
        }
        problems.extend(backfill_problems(&Backfill {
            ordinal,
            seed_ordinal: plan.seed_ordinal,
            table: &run.table,
            accepted: &history.prior_sets_at(ordinal)?,
            prior: &prior,
            batch: &batch,
            seed_extension: &review.seed_extension,
        }));
        prior = PriorSets {
            ordinal,
            regressions: run.table.regressions(),
            closures: run.table.closures(),
        };
    }
    if !problems.is_empty() {
        write_report(out, 1, &problems)?;
        return Ok(Verdict::Fail(problems));
    }
    let tables = Tables {
        hashes: BTreeMap::from([("outcomes", canonical_sha256(&tables)?)]),
    };
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
    let sealed = receipt(
        &ReceiptInputs {
            plan: &plan,
            main: &main,
            candidate: None,
            predecessor: Some(&predecessor),
            universe: &universe,
            tables: &tables,
            seed_ledger_sha256,
            contexts: predecessor.context_roster_hash.clone(),
        },
        ReceiptKind::Amendment,
        predecessor.checkpoint_ordinal,
        &sets,
        0,
    )?;
    sealed.write(&out.join("receipt.json"))?;
    history.publish(&sealed, &sets, None)?;
    Ok(Verdict::Pass)
}

/// An amendment's checked inputs.
struct Amendment {
    history: History,
    plan: HistoryPlan,
    predecessor: Receipt,
    review: AmendmentReview,
    universe: UniverseState,
    batch: BTreeSet<String>,
    captures: Vec<RetainedCapture>,
    main: Capture,
}

/// Checks an amendment's review, its batch and the retained captures.
fn amendment(history_dir: &Path, previous: &Path, review: &Path) -> Result<Amendment, DiffError> {
    let history = History::new(history_dir);
    let plan = history.plan()?;
    let predecessor = trusted_predecessor(&history, previous)?;
    let review: AmendmentReview = read_json(review)?;
    if review.format_version != FORMAT_VERSION || review.reviewer != plan.reviewer {
        return Err(DiffError::Identity(
            "the amendment is not reviewed by the plan's reviewer".into(),
        ));
    }
    let universe = {
        let _lock = history.universe.lock()?;
        history.universe.load()?
    };
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
    Ok(Amendment {
        history,
        plan,
        predecessor,
        review,
        universe,
        batch,
        captures,
        main,
    })
}

/// Compares main with the retained checkpoint `entry` over the expanded
/// universe, writing its tables under `out`; returns the run and the hash
/// of its outcome table.
fn backfill_run(
    history: &History,
    main: &Capture,
    entry: &RetainedCapture,
    universe: &UniverseState,
    live: &LiveOptions,
    out: &Path,
) -> Result<(ComparisonRun, String), DiffError> {
    let ordinal = entry.ordinal;
    let candidate = open_capture(&entry.path)?;
    if candidate.hash != entry.capture_hash {
        return Err(DiffError::Identity(format!(
            "checkpoint {ordinal} capture changed"
        )));
    }
    let (pair, run) = compare_captures(
        main,
        &candidate,
        universe,
        history.universe.blobs(),
        live,
        &format!("attempt:backfill-{ordinal}"),
    )?;
    let changes = artifact_changes(&pair, &run, &[])?;
    let written = write_tables(
        &out.join(format!("checkpoint-{ordinal:02}")),
        &run,
        &changes,
    )?;
    let outcomes = written.hashes.get("outcomes").cloned().unwrap_or_default();
    Ok((run, outcomes))
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

/// Why a backfilled checkpoint breaks the reconstructed history: checkpoint
/// 1 must still equal main, the seed may grow only by exactly the reviewed,
/// newly journaled witnesses, later checkpoints may only shrink, and no
/// earned closure may be lost.
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
    let mut problems = Vec::new();
    let regressions = table.regressions();
    if *ordinal == 1 {
        if table
            .outcomes
            .iter()
            .any(|outcome| !matches!(outcome.class, Class::Identical | Class::Inherited))
        {
            problems.push("checkpoint 1 no longer equals main".into());
        }
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
        let reviewed = row_keys(seed_extension);
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
    for lost in accepted.closures.difference(&table.closures()) {
        problems.push(format!("checkpoint {ordinal} loses earned closure {lost}"));
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
    /// The sets of the accepted receipt at `ordinal`.
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

/// Verifies a receipt against its history: its seal, its predecessor chain
/// back to genesis, its sets, and the universe journal it names.
///
/// # Errors
///
/// [`DiffError::Identity`] on any broken link.
pub fn verify_history(history_dir: &Path, receipt_path: &Path) -> Result<Verdict, DiffError> {
    let history = History::new(history_dir);
    let plan = history.plan()?;
    let universe = history.universe.load()?;
    let mut receipts = BTreeMap::new();
    let dir = history.root.join("receipts");
    for entry in std::fs::read_dir(&dir).map_err(|error| io(&dir, &error))? {
        let entry = entry.map_err(|error| io(&dir, &error))?;
        let receipt = Receipt::read(&entry.path())?;
        receipts.insert(receipt.seal_sha256.clone(), receipt);
    }
    let mut current = Receipt::read(receipt_path)?;
    loop {
        if current.history_id != plan.history_id {
            return Err(DiffError::Identity(
                "a receipt belongs to another history".into(),
            ));
        }
        history.prior_sets(&current)?;
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
        {
            return Err(DiffError::Identity(format!(
                "receipt {} does not follow {predecessor}",
                current.seal_sha256
            )));
        }
        current = previous;
    }
}

/// The `Value` of a ledger with `rows`, for writing by orchestration code.
#[must_use]
pub fn ledger_value(rows: &[LedgerRow]) -> Value {
    json!(rows)
}

/// The outcome table of a comparison output directory.
///
/// # Errors
///
/// [`DiffError::Identity`] when `outcomes.jsonl` is missing or malformed.
pub fn read_outcomes(out: &Path) -> Result<OutcomeTable, DiffError> {
    let path = out.join("outcomes.jsonl");
    let text = std::fs::read_to_string(&path).map_err(|error| io(&path, &error))?;
    let outcomes = text
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OutcomeTable::new(outcomes))
}
