//! One captured revision: the binaries the capture built from it, the
//! producer dump, the contract export, CLI parity, the admission cells and
//! the observation controls, each verified against the others.
//!
//! [`capture`] builds every binary itself ([`super::build`]), so a stale or
//! foreign binary cannot stand in for the revision. [`open_capture`] rechecks
//! every hash the manifest records and every binary the build receipt names;
//! logs, run directories and scratch are diagnostics and are not bound.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::DiffError;
use super::admission::{self, AdmissionRecord};
use super::build::{self, BuildReceipt, dir_sha256};
use super::capture::VerifiedDump;
use super::controls::{
    ControlBlobs, ControlCase, ControlRegistry, DescribeResponse, ObservationRecord, QUERY_KINDS,
    Runner, copy_registry, observe_case,
};
use super::history::{FORMAT_VERSION, canonical_json, canonical_sha256, write_new};
use crate::admission::SUITE_DIR;
use crate::helm::adjudication::KUBERNETES_RELEASE;
use crate::manifest::BuildProvenance;
use crate::registry::{self, GenerationRecipe, PROVIDER_BUNDLE, PolicyRecipe};
use crate::source_digest::sha256_hex;

/// The capture scope this version writes and reads.
pub const SCOPE: &str = "construction-v1";

/// The capture manifest: one revision's build, dump, export, parity,
/// admission and controls, with its source and Helm provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureManifest {
    /// [`FORMAT_VERSION`].
    pub format_version: u32,
    /// [`SCOPE`].
    pub scope: String,
    /// The checkout's commit.
    pub commit: String,
    /// The commit's tree.
    pub tree: String,
    /// The checkout's absolute path when captured; nothing reads it later.
    pub checkout: String,
    /// `git status --porcelain` of the checkout; empty unless synthetic.
    pub status: String,
    /// Whether the capture was taken from a dirty tree for a synthetic
    /// demonstration; synthetic captures never enter a construction history.
    pub synthetic: bool,
    /// SHA-256 of `bin/build.json`.
    pub build_sha256: String,
    /// The generation provenance every binary embeds.
    pub provenance: BuildProvenance,
    /// SHA-256 of the dump's manifest.
    pub dump_manifest_sha256: String,
    /// SHA-256 of `export/export.json`.
    pub export_sha256: String,
    /// SHA-256 of `parity.jsonl`.
    pub parity_sha256: String,
    /// [`dir_sha256`] of the admission suite.
    pub admission_suite_sha256: String,
    /// SHA-256 of `admission.jsonl`.
    pub admission_sha256: String,
    /// [`dir_sha256`] of the provider bundle.
    pub provider_sha256: String,
    /// SHA-256 of the helmsweep executable.
    pub helmsweep_sha256: String,
    /// helmsweep's version report.
    pub helmsweep_version: String,
    /// SHA-256 of the pinned Helm executable that regrounds controls.
    pub helm_sha256: String,
    /// The pinned Kubernetes release.
    pub kubernetes_release: String,
    /// SHA-256 of the dump's registry entries and recipe inputs.
    pub suite_sha256: String,
    /// SHA-256 of the context roster's exact bytes, when one was captured.
    pub contexts_sha256: Option<String>,
    /// The observation controls.
    pub controls: ControlsCapture,
}

/// What a capture's `controls/` directory must hash to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlsCapture {
    /// SHA-256 of the copied registry's manifest.
    pub registry: String,
    /// SHA-256 of the runner executable: its build id.
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
    /// The observations by witness id.
    pub observations: BTreeMap<String, ObservationRecord>,
}

/// The contract exporter's `export.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportManifest {
    /// Version 1.
    pub format_version: u32,
    /// The exporter's build.
    pub build: BuildProvenance,
    /// One entry per whole-chart recipe, sorted by key.
    pub entries: Vec<ExportEntry>,
}

/// One exported recipe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportEntry {
    /// The registry key.
    pub key: String,
    /// The contract document.
    pub contract: ExportFile,
    /// The session's schema.
    pub schema: ExportFile,
}

/// One exported file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportFile {
    /// Its name inside the export directory.
    pub file: String,
    /// SHA-256 of its bytes.
    pub sha256: String,
}

/// One exported whole-chart contract document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedContract {
    /// The registry key.
    pub key: String,
    /// Its file inside `export/`.
    pub file: PathBuf,
    /// SHA-256 of its bytes.
    pub sha256: String,
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
    /// Its verified controls.
    pub controls: CapturedControls,
    /// Its exported contract documents by key.
    pub contracts: BTreeMap<String, ExportedContract>,
    /// Its admission verdicts.
    pub admissions: Vec<AdmissionRecord>,
}

/// What `capture` records.
pub struct CaptureRequest {
    /// The revision's checkout.
    pub checkout: PathBuf,
    /// The commit the checkout must be at.
    pub commit: String,
    /// The pinned helmsweep executable.
    pub helmsweep: PathBuf,
    /// The pinned Helm executable that regrounds the controls.
    pub helm: PathBuf,
    /// The Cargo target directory the capture builds in.
    pub build_target: PathBuf,
    /// The control registry.
    pub controls: PathBuf,
    /// The context roster to retain, if any.
    pub contexts: Option<PathBuf>,
    /// The new capture directory.
    pub out: PathBuf,
    /// Allows a dirty tree, marking the capture synthetic.
    pub synthetic: bool,
}

pub(crate) fn io(path: &Path, error: &std::io::Error) -> DiffError {
    DiffError::Infrastructure(format!("{}: {error}", path.display()))
}

/// Runs `program` in `cwd`, appending its record to `log`; returns stdout.
pub(crate) fn run(
    program: &Path,
    args: &[&str],
    cwd: &Path,
    log: &Path,
) -> Result<String, DiffError> {
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

/// SHA-256 of the file at `path`.
pub(crate) fn file_sha256(path: &Path) -> Result<String, DiffError> {
    std::fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| io(path, &error))
}

pub(crate) fn json_line<T: Serialize>(value: &T) -> Result<String, DiffError> {
    let value = serde_json::to_value(value)
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))?;
    String::from_utf8(canonical_json(&value))
        .map_err(|error| DiffError::Infrastructure(format!("serialize: {error}")))
}

pub(crate) fn write_lines(path: &Path, lines: &[String]) -> Result<String, DiffError> {
    let mut bytes = Vec::new();
    for line in lines {
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
    }
    write_new(path, &bytes)?;
    Ok(sha256_hex(&bytes))
}

fn read_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, DiffError> {
    let bytes = std::fs::read(path).map_err(|error| io(path, &error))?;
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_slice(line)
                .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))
        })
        .collect()
}

/// The checkout's tree and status, refused unless it is at `commit` and,
/// outside a synthetic capture, clean.
fn checked_head(
    checkout: &Path,
    commit: &str,
    synthetic: bool,
    log: &Path,
) -> Result<(String, String), DiffError> {
    let git = Path::new("git");
    let head = run(git, &["rev-parse", "HEAD"], checkout, log)?;
    if head != commit {
        return Err(DiffError::Identity(format!(
            "{} is at {head}, not {commit}",
            checkout.display()
        )));
    }
    let tree = run(git, &["rev-parse", "HEAD^{tree}"], checkout, log)?;
    let status = run(git, &["status", "--porcelain"], checkout, log)?;
    if !status.is_empty() && !synthetic {
        return Err(DiffError::Identity(format!(
            "{} has uncommitted changes:\n{status}",
            checkout.display()
        )));
    }
    Ok((tree, status))
}

/// Measures the admission suite with the built CLI into `admission.jsonl`;
/// returns its hash.
fn record_admissions(
    suite_root: &Path,
    testdata: &Path,
    bin: &Path,
    out: &Path,
) -> Result<String, DiffError> {
    let admissions = admission::measure(
        suite_root,
        testdata,
        &bin.join("helm-schema"),
        &out.join("admission-scratch"),
    )?;
    let lines = admissions
        .iter()
        .map(json_line)
        .collect::<Result<Vec<_>, _>>()?;
    write_lines(&out.join("admission.jsonl"), &lines)
}

/// Copies `from` to `to`; returns the copy's hash.
fn copy_hashed(from: &Path, to: &Path) -> Result<String, DiffError> {
    std::fs::copy(from, to).map_err(|error| io(from, &error))?;
    file_sha256(to)
}

/// Captures one revision: builds its binaries, runs its producer, exporter,
/// CLI, admission cells and control runner, and verifies each against the
/// others.
///
/// # Errors
///
/// [`DiffError::Identity`] when the checkout is not at `commit`, is dirty
/// without `synthetic`, changes while captured, or one job's output
/// disagrees with another's; [`DiffError::Infrastructure`] when a process or
/// file fails.
pub fn capture(request: &CaptureRequest) -> Result<Capture, DiffError> {
    let CaptureRequest {
        checkout,
        commit,
        helmsweep,
        helm,
        build_target,
        controls,
        contexts,
        out,
        synthetic,
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
    let (tree, status) = checked_head(checkout, commit, *synthetic, &log)?;
    let tree_before = build::tree_sha256(checkout)?;
    let bin = out.join("bin");
    let provenance =
        build::build(checkout, build_target, &bin, &logs.join("build.log"))?.provenance;
    let dump_dir = out.join("dump");
    let dump_arg = dump_dir.to_string_lossy().into_owned();
    let producer = bin.join("corpus_generation");
    run(
        &producer,
        &["--out", &dump_arg, "--helm-ready"],
        checkout,
        &log,
    )?;
    run(&producer, &["--verify", &dump_arg], checkout, &log)?;
    let dump = VerifiedDump::open(&dump_dir)?;
    if *dump.build() != provenance {
        return Err(DiffError::Identity(format!(
            "the producer embeds {:?}, the build {provenance:?}",
            dump.build()
        )));
    }
    let export_dir = out.join("export");
    run(
        &bin.join("contract_export"),
        &["--out", &export_dir.to_string_lossy()],
        checkout,
        &log,
    )?;
    verify_export(&export_dir, &dump, &provenance)?;
    let testdata = checkout.join("testdata");
    let parity = cli_parity(
        &bin.join("helm-schema"),
        &testdata,
        &dump,
        &out.join("parity"),
    )?;
    let parity_sha256 = write_lines(&out.join("parity.jsonl"), &parity)?;
    let suite_root = testdata.join(SUITE_DIR);
    let admission_sha256 = record_admissions(&suite_root, &testdata, &bin, out)?;
    let controls = record_controls(out, controls, &provenance, helm)?;
    let helmsweep_version = run(helmsweep, &["version"], checkout, &log)?;
    let contexts_sha256 = contexts
        .as_deref()
        .map(|path| copy_hashed(path, &out.join("contexts.json")))
        .transpose()?;
    if build::tree_sha256(checkout)? != tree_before {
        return Err(DiffError::Identity(format!(
            "{} changed during the capture",
            checkout.display()
        )));
    }
    let manifest = CaptureManifest {
        format_version: FORMAT_VERSION,
        scope: SCOPE.into(),
        commit: commit.clone(),
        tree,
        checkout: checkout.to_string_lossy().into_owned(),
        status,
        synthetic: *synthetic,
        build_sha256: file_sha256(&bin.join("build.json"))?,
        provenance,
        dump_manifest_sha256: dump.manifest_sha256().to_string(),
        export_sha256: file_sha256(&export_dir.join("export.json"))?,
        parity_sha256,
        admission_suite_sha256: dir_sha256(&suite_root)?,
        admission_sha256,
        provider_sha256: dir_sha256(&testdata.join(PROVIDER_BUNDLE))?,
        helmsweep_sha256: file_sha256(helmsweep)?,
        helmsweep_version,
        helm_sha256: file_sha256(helm)?,
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

/// Checks the exporter's manifest: its provenance, one entry per
/// whole-chart recipe the dump holds (the producer always dumps every
/// registered one), every file's hash, and each schema equal to the
/// producer's artifact.
fn verify_export(
    dir: &Path,
    dump: &VerifiedDump,
    provenance: &BuildProvenance,
) -> Result<BTreeMap<String, ExportedContract>, DiffError> {
    let path = dir.join("export.json");
    let manifest: ExportManifest = serde_json::from_slice(
        &std::fs::read(&path)
            .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))?,
    )
    .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))?;
    if manifest.build != *provenance || manifest.format_version != 1 {
        return Err(DiffError::Identity(
            "the exporter is not this capture's build".into(),
        ));
    }
    let dumped = dump.keys().collect::<BTreeSet<_>>();
    let expected = registry::registry()
        .into_iter()
        .filter(|spec| matches!(spec.recipe, GenerationRecipe::Chart(_)))
        .map(|spec| spec.id.key())
        .filter(|key| dumped.contains(key.as_str()))
        .collect::<BTreeSet<_>>();
    let mut contracts = BTreeMap::new();
    for ExportEntry {
        key,
        contract,
        schema,
    } in manifest.entries
    {
        let file = |field: &str, exported: &ExportFile| -> Result<(PathBuf, String), DiffError> {
            if !super::controls::is_normal_relative(&exported.file)
                || file_sha256(&dir.join(&exported.file))? != exported.sha256
            {
                return Err(DiffError::Identity(format!(
                    "export {key}: {field} file mismatch"
                )));
            }
            Ok((PathBuf::from(&exported.file), exported.sha256.clone()))
        };
        let (contract_file, contract_sha) = file("contract", &contract)?;
        let (schema_file, schema_sha) = file("schema", &schema)?;
        let produced = dump.schema(&key)?;
        if sha256_hex(&produced.bytes) != schema_sha {
            return Err(DiffError::Identity(format!(
                "export {key}: the session's schema differs from the producer's {}",
                schema_file.display()
            )));
        }
        let previous = contracts.insert(
            key.clone(),
            ExportedContract {
                key,
                file: contract_file,
                sha256: contract_sha,
            },
        );
        if previous.is_some() {
            return Err(DiffError::Identity("the export lists a key twice".into()));
        }
    }
    if contracts.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err(DiffError::Identity(
            "the export does not cover exactly the whole-chart recipes".into(),
        ));
    }
    Ok(contracts)
}

/// The CLI arguments equivalent to a final-output `recipe`, with
/// `<chart>`, `<bundle>`, `<override>` and `<out>` placeholders.
fn cli_arguments(recipe: &PolicyRecipe) -> Vec<String> {
    use helm_schema::generation::{DeclaredTypes, RootPolicy, SchemaProfile};
    use helm_schema::output::ReferencePolicy;
    let chart = &recipe.chart;
    let mut arguments = [
        "<chart>",
        "--no-config",
        "--k8s-version",
        chart.k8s_version,
        "--offline",
        "--k8s-schema-cache-dir",
        "<bundle>/kubernetes-json-schema-cache",
        "--crd-catalog-cache-dir",
        "<bundle>/crds-catalog-cache",
        "--profile",
        match chart.profile {
            SchemaProfile::Full => "full",
            SchemaProfile::Lean => "lean",
        },
        "--output-layout",
        "pretty",
    ]
    .map(str::to_string)
    .to_vec();
    let mut flag = |enabled: bool, name: &str| {
        if enabled {
            arguments.push(name.to_string());
        }
    };
    flag(!chart.include_tests, "--exclude-tests");
    flag(!chart.include_subchart_values, "--no-subchart-values");
    flag(chart.infer_required, "--infer-required");
    flag(!chart.minimize, "--no-minimize");
    flag(chart.authoring.root == RootPolicy::Open, "--open-root");
    match chart.authoring.declared_types {
        DeclaredTypes::Assert => {}
        DeclaredTypes::Annotate => {
            arguments.extend(["--declared-types".to_string(), "annotate".to_string()]);
        }
    }
    match recipe.reference_policy {
        ReferencePolicy::SelfContained => {}
        ReferencePolicy::PreserveRefs => arguments.push("--keep-refs".to_string()),
        ReferencePolicy::FullyInlinedExport => arguments.push("--inline-refs".to_string()),
    }
    for values in chart.values_files {
        arguments.extend(["--values".to_string(), format!("<testdata>/{values}")]);
    }
    if recipe.override_file.is_some() {
        arguments.extend(["--override-schema".to_string(), "<override>".to_string()]);
    }
    arguments.extend(["--output".to_string(), "<out>".to_string()]);
    arguments
}

/// Runs the CLI for every final-output recipe and requires byte equality
/// with the producer's artifact; returns one parity line per recipe.
/// Whole-chart library recipes stay producer-only comparisons.
fn cli_parity(
    cli: &Path,
    testdata: &Path,
    dump: &VerifiedDump,
    scratch: &Path,
) -> Result<Vec<String>, DiffError> {
    std::fs::create_dir_all(scratch).map_err(|error| io(scratch, &error))?;
    let mut lines = Vec::new();
    for (index, spec) in registry::registry().into_iter().enumerate() {
        let GenerationRecipe::FinalPolicy(recipe) = spec.recipe else {
            continue;
        };
        let key = spec.id.key();
        let out = scratch.join(format!("{index:04}.schema.json"));
        let override_path = scratch.join(format!("{index:04}.override.json"));
        if let Some(file) = recipe.override_file {
            write_new(&override_path, file.contents.as_bytes())?;
        }
        let home = scratch.join(format!("{index:04}-home"));
        std::fs::create_dir_all(&home).map_err(|error| io(&home, &error))?;
        let arguments = cli_arguments(&recipe)
            .into_iter()
            .map(|argument| {
                argument
                    .replace(
                        "<chart>",
                        &testdata
                            .join("charts")
                            .join(recipe.chart.chart)
                            .to_string_lossy(),
                    )
                    .replace(
                        "<bundle>",
                        &testdata.join(PROVIDER_BUNDLE).to_string_lossy(),
                    )
                    .replace("<testdata>", &testdata.to_string_lossy())
                    .replace("<override>", &override_path.to_string_lossy())
                    .replace("<out>", &out.to_string_lossy())
            })
            .collect::<Vec<_>>();
        let output = std::process::Command::new(cli)
            .env_clear()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &home)
            .env("XDG_CACHE_HOME", &home)
            .args(&arguments)
            .output()
            .map_err(|error| io(cli, &error))?;
        if !output.status.success() {
            return Err(DiffError::Infrastructure(format!(
                "CLI parity {key}: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        let cli_bytes = std::fs::read(&out).map_err(|error| io(&out, &error))?;
        let produced = dump.schema(&key)?;
        if cli_bytes != produced.bytes {
            return Err(DiffError::Identity(format!(
                "CLI parity {key}: the CLI's schema differs from the producer's"
            )));
        }
        lines.push(json_line(&json!({
            "key": key,
            "argv": cli_arguments(&recipe),
            "sha256": sha256_hex(&cli_bytes),
        }))?);
    }
    if lines.is_empty() {
        return Err(DiffError::Identity(
            "the registry has no final-output recipe to check CLI parity on".into(),
        ));
    }
    Ok(lines)
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

/// Opens and verifies the capture at `root`: every binary against the build
/// receipt, the dump, the export, the parity and admission records and the
/// controls against the manifest.
///
/// # Errors
///
/// [`DiffError::Identity`] when anything disagrees with the manifest.
pub fn open_capture(root: &Path) -> Result<Capture, DiffError> {
    let path = root.join("capture.json");
    let bytes = std::fs::read(&path).map_err(|error| io(&path, &error))?;
    let manifest: CaptureManifest = serde_json::from_slice(&bytes)
        .map_err(|error| DiffError::Identity(format!("{}: {error}", path.display())))?;
    if manifest.format_version != FORMAT_VERSION || manifest.scope != SCOPE {
        return Err(DiffError::Identity(format!(
            "{} is not a version-1 {SCOPE} capture",
            path.display()
        )));
    }
    let stale = || {
        DiffError::Identity(format!(
            "{} no longer matches its capture manifest",
            root.display()
        ))
    };
    let bin = root.join("bin");
    let receipt: BuildReceipt = serde_json::from_slice(
        &std::fs::read(bin.join("build.json")).map_err(|error| io(&bin, &error))?,
    )
    .map_err(|error| DiffError::Identity(format!("build.json: {error}")))?;
    build::verify(&bin, &receipt)?;
    if file_sha256(&bin.join("build.json"))? != manifest.build_sha256
        || receipt.provenance != manifest.provenance
    {
        return Err(stale());
    }
    let dump = VerifiedDump::open(&root.join("dump"))?;
    if dump.manifest_sha256() != manifest.dump_manifest_sha256
        || *dump.build() != manifest.provenance
        || suite_sha256(&dump)? != manifest.suite_sha256
        || file_sha256(&root.join("export/export.json"))? != manifest.export_sha256
        || file_sha256(&root.join("parity.jsonl"))? != manifest.parity_sha256
        || file_sha256(&root.join("admission.jsonl"))? != manifest.admission_sha256
    {
        return Err(stale());
    }
    let contracts = verify_export(&root.join("export"), &dump, &manifest.provenance)?;
    let admissions = read_lines(&root.join("admission.jsonl"))?;
    let controls = open_controls(&root.join("controls"), &bin, &manifest.controls)?;
    if let Some(expected) = &manifest.contexts_sha256
        && file_sha256(&root.join("contexts.json"))? != *expected
    {
        return Err(stale());
    }
    Ok(Capture {
        root: root.to_path_buf(),
        hash: sha256_hex(&bytes),
        manifest,
        dump,
        controls,
        contracts,
        admissions,
    })
}

/// Copies the registry into `out/controls`, regrounds every case with the
/// pinned Helm `helm`, checks the runner was built as `provenance`, and
/// records one observation per scored case.
///
/// # Errors
///
/// [`DiffError::Identity`] when the registry misses a query kind, a
/// grounding does not reproduce, or the runner was built from another source
/// or answers another request; [`DiffError::Infrastructure`] when an
/// invocation fails.
pub fn record_controls(
    out: &Path,
    registry: &Path,
    provenance: &BuildProvenance,
    helm: &Path,
) -> Result<ControlsCapture, DiffError> {
    let dir = out.join("controls");
    let registry = copy_registry(&ControlRegistry::open(registry)?, &dir.join("registry"))?;
    let covered = registry
        .cases
        .values()
        .filter_map(ControlCase::scored_query)
        .map(super::controls::Query::kind)
        .collect::<BTreeSet<_>>();
    if covered != QUERY_KINDS.iter().copied().collect() {
        return Err(DiffError::Identity(format!(
            "the control registry scores {covered:?}, not every version-1 query"
        )));
    }
    let helm_sha256 = file_sha256(helm)?;
    for case in registry.cases.values() {
        reground(
            case,
            &registry.blobs(),
            helm,
            &helm_sha256,
            &dir.join("reground").join(&case.id),
        )?;
    }
    let runner = Runner::new(&out.join("bin/protocol-runner"))?;
    let describe = runner.describe(&dir.join("runs/describe"))?;
    if describe.build != *provenance {
        return Err(DiffError::Identity(format!(
            "the control runner was built from {:?}, the producer from {provenance:?}",
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
        let run_dir = dir.join("runs").join(&case.id);
        let blob_root = dir.join("inputs").join(&case.id);
        super::controls::stage_blobs(&registry.blobs(), case, &blob_root)?;
        let record = observe_case(&runner, &run_dir, &blob_root, case)?;
        lines.push(json_line(&record)?);
    }
    Ok(ControlsCapture {
        registry: registry.sha256,
        runner: runner.build_id,
        describe: sha256_hex(describe_bytes.as_bytes()),
        observations: write_lines(&dir.join("observations.jsonl"), &lines)?,
    })
}

/// Reruns `case`'s grounding invocation with the pinned Helm on a staged
/// copy of its chart and requires the recorded exit, stdout and stderr.
fn reground(
    case: &ControlCase,
    blobs: &ControlBlobs,
    helm: &Path,
    helm_sha256: &str,
    scratch: &Path,
) -> Result<(), DiffError> {
    let evidence = case.check_grounding(blobs)?;
    if evidence.helm_sha256 != helm_sha256 {
        return Err(DiffError::Identity(format!(
            "{}: grounded by another Helm binary",
            case.id
        )));
    }
    let output = super::controls::StagedChart::new(blobs, &case.input, scratch)?
        .run(helm, &evidence.argv)?;
    let reproduced = output.status.code() == Some(evidence.exit_code)
        && sha256_hex(&output.stdout) == evidence.stdout_sha256
        && sha256_hex(&output.stderr) == evidence.stderr_sha256;
    if !reproduced {
        return Err(DiffError::Identity(format!(
            "{}: the pinned Helm no longer reproduces its grounding (exit {:?})",
            case.id,
            output.status.code()
        )));
    }
    Ok(())
}

/// Opens a capture's controls and checks them against its manifest.
fn open_controls(
    dir: &Path,
    bin: &Path,
    expected: &ControlsCapture,
) -> Result<CapturedControls, DiffError> {
    let registry = ControlRegistry::open(&dir.join("registry"))?;
    let runner = Runner::new(&bin.join("protocol-runner"))
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
    for record in read_lines::<ObservationRecord>(&dir.join("observations.jsonl"))? {
        if record.response.runner_build_id != runner.build_id {
            return Err(DiffError::Identity(format!(
                "{}: answered by another runner",
                record.case
            )));
        }
        observations.insert(record.witness.clone(), record);
    }
    Ok(CapturedControls {
        registry,
        runner,
        describe,
        observations,
    })
}

/// Re-evaluates `case` with `capture`'s retained runner on a fresh flat
/// blob root under the new directory `scratch`, staged from `source`: no
/// checkout, current runner or Helm answer is used.
///
/// # Errors
///
/// [`DiffError::Identity`] when the retained runner changed or a blob is
/// missing; otherwise as [`observe_case`].
pub fn replay_observation(
    capture: &Capture,
    source: &ControlBlobs,
    case: &ControlCase,
    scratch: &Path,
) -> Result<ObservationRecord, DiffError> {
    let runner = Runner::new(&capture.controls.runner.path)
        .map_err(|error| DiffError::Identity(format!("retained runner: {error}")))?;
    if runner.build_id != capture.controls.runner.build_id {
        return Err(DiffError::Identity("the retained runner changed".into()));
    }
    if scratch.exists() {
        return Err(DiffError::Identity(format!(
            "{} already exists",
            scratch.display()
        )));
    }
    let blob_root = scratch.join("blobs");
    super::controls::stage_blobs(source, case, &blob_root)?;
    observe_case(&runner, &scratch.join("run"), &blob_root, case)
}
