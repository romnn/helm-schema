//! `cell_matrix`: reproducible verdict matrices over (chart, values, Kubernetes version) cells.
//!
//! For every cell it records what each labelled `helm-schema` binary's schema says about the
//! values Helm itself coalesces, what pinned Helm v4.2.3 does with the same original values
//! file, and whether the render is valid under the pinned offline Kubernetes bundle. Every
//! column measures the *prepared* chart ([`PinnedHelmChart::prepare_for_explicit_versions`]:
//! shipped `values.schema.json` files and `templates/tests` removed, `--skip-schema-validation`).
//! The outputs are deterministic and path-free, so `--check` can reproduce them byte for
//! byte. Design: round8-runner-evidence/proposals/cell-matrix.md (v3).
//!
//! Three Kubernetes versions answer three questions, and the table header states each:
//! - a cell's `kube_version` is what Helm renders it at (`--kube-version`);
//! - `--gen-k8s-version` is what every schema is generated at, for every cell;
//! - `--validate-release` is the bundle the `k8s` column validates against. `k8s=valid` means
//!   "valid under that bundle", not "valid on the cell's Kubernetes". A verdict for a cell at another
//!   minor version than the bundle's is marked with the bundle's, e.g. `valid@1.29`.
//!
//! A reviewer reproduces a hand-off with
//! `cell_matrix --cells <cells.tsv> --bin <LABEL>=<PATH>@<COMMIT>... --helm <helm> --helmsweep <helmsweep>
//! --out <dir> --scratch <dir> --check --require-expected`; exit 0 means the committed outputs were
//! reproduced byte for byte and every claimed `(cell, column, expected)` held. `--scratch` names an
//! existing writable directory prepared beforehand (for example `$TMPDIR/cm-check`, made with `mkdir`
//! by whoever sets up the sandbox; the tool never creates it): everything the run writes, the Helm replay
//! store included, stays there, so nothing is written under `--out` or the checkout. Without it, scratch
//! lives in the target named by `CARGO_TARGET_DIR`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde_json::{Value, json};
use test_util::helm_values::{ValuesError, ValuesOptions, coalesce_chart_values};
use test_util::scratch::ScratchDir;

use crate::helm::adjudication::{
    KubernetesVerdict, OfflineKubernetesValidator, PinnedHelmChart, ViolationKey, decode,
};
use crate::helm::invocation::{
    HelmExecution, HelmRunner, Replay, TimedOut, output_within, tree_sha256,
};
use crate::source_digest::sha256_hex;

/// The committed provider bundle's content identities ([`tree_sha256`]); verified before any
/// generation or validation, so a wrong bundle is refused even when it does not change.
pub const KUBERNETES_BUNDLE_SHA256: &str =
    "f0544d9487290b92c8d43cdf4b48e93b816d94f2ee26e4d57ffae66ade5b815b";
/// See [`KUBERNETES_BUNDLE_SHA256`].
pub const CRD_BUNDLE_SHA256: &str =
    "73dd3a9ff91d5905162021f6926a6190f88b9798a9ca849d5be2655c29815700";

/// The table format; changing columns, header lines or encodings must change it.
const FORMAT: &str = "cell_matrix v1";
/// The gate columns a cell may carry expectations for, besides the binary labels.
const HELM_COLUMNS: [&str; 3] = ["helm_rc", "helm_class", "k8s"];

/// A run's inputs, as the command line gives them.
#[derive(Debug)]
pub struct Options {
    /// The cells file; charts and `@file` values are relative to its directory.
    pub cells: PathBuf,
    /// The labelled binaries, in command-line order.
    pub bins: Vec<Bin>,
    /// The pinned Helm executable.
    pub helm: PathBuf,
    /// The helmsweep executable whose `classify` names Helm's class.
    pub helmsweep: PathBuf,
    /// The provider bundle directory (`kubernetes-json-schema-cache`, `crds-catalog-cache`).
    pub bundle: PathBuf,
    /// `--k8s-version` of every schema generation.
    pub gen_k8s_version: String,
    /// `--profile` of every schema generation.
    pub profile: String,
    /// The Kubernetes release the rendered resources are validated against.
    pub validate_release: String,
    /// Where the three output files live.
    pub out: PathBuf,
    /// Compare with the files in `out` instead of writing them.
    pub check: bool,
    /// Every cell must carry expectations, and they must hold.
    pub require_expected: bool,
    /// How long one generation or one external command (Helm, helmsweep) may run: longer is a
    /// harness failure (exit 3).
    pub timeout: Duration,
    /// An existing writable directory to hold every scratch file (the Helm replay store included,
    /// whatever `SCHEMA_HELM_INVOCATION_CACHE` says) instead of the target's `scratch/`, so a reviewer
    /// can `--check` from a read-only checkout; never under `out`, and never created by the tool.
    pub scratch: Option<PathBuf>,
}

/// One `--bin LABEL=PATH@COMMIT`.
#[derive(Clone, Debug)]
pub struct Bin {
    /// The column name.
    pub label: String,
    /// The executable.
    pub path: PathBuf,
    /// The 40-hex commit it was built from.
    pub commit: String,
}

/// An input the tool refuses (exit 2): nothing is written.
#[derive(Debug, thiserror::Error)]
#[error("refused: {0}")]
pub struct Refusal(pub String);

/// How a completed run ends.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The outputs were written, or `--check` reproduced them, and every expectation held.
    Ok,
    /// `--check` found other bytes, or an expectation did not hold; the reasons.
    Mismatch(Vec<String>),
}

fn refuse(message: impl Into<String>) -> eyre::Report {
    Refusal(message.into()).into()
}

/// One cells.tsv row.
struct Cell {
    id: String,
    /// As written, relative to the cells file.
    chart: String,
    values_bytes: Vec<u8>,
    kube_version: String,
    /// `COLUMN=VALUE` pairs, `None` for `-`.
    expect: Option<Vec<(String, String)>>,
}

/// The three output files' names and bytes.
struct Outputs {
    files: Vec<(&'static str, Vec<u8>)>,
    mismatches: Vec<String>,
}

/// Runs the matrix.
///
/// # Errors
///
/// A [`Refusal`] for an unacceptable input, any other error for a harness failure (a
/// crashing binary, an uncompilable schema, a Helm execution failure).
pub fn run(options: &Options) -> eyre::Result<Outcome> {
    match &options.scratch {
        Some(dir) => use_scratch(dir, &options.out)?,
        None => check_scratch_root()?,
    }
    let options = &resolved(options)?;
    let release_minor = bundle_release(
        &options.bundle,
        "--validate-release",
        &options.validate_release,
    )?;
    bundle_release(
        &options.bundle,
        "--gen-k8s-version",
        &options.gen_k8s_version,
    )?;
    let base = options
        .cells
        .parent()
        .ok_or_eyre("the cells file has no directory")?
        .to_path_buf();
    let cells = read_cells(&options.cells, &base, options)?;
    verify_bundle(&options.bundle)?;
    let bins = identify_bins(options)?;
    if options.check {
        check_recorded_bins(options, &bins)?;
    }
    let runner = helm_runner(&options.helm, options.timeout, options.scratch.is_some())?;
    let helmsweep = program_identity(&options.helmsweep, &["version"], options.timeout)?;

    let charts = prepare_charts(options, &cells, &base, runner)?;
    let scratch = ScratchDir::new("cell-matrix")?;
    let mut schemas = BTreeMap::new();
    let mut generations = BTreeMap::new();
    for bin in &bins {
        for (name, chart) in &charts {
            let (validator, generated) = generate(options, bin, chart, scratch.path())
                .wrap_err_with(|| {
                    let ids: Vec<&str> = cells
                        .iter()
                        .filter(|cell| cell.chart == *name)
                        .map(|cell| cell.id.as_str())
                        .collect();
                    format!("generating chart {name} (cells {})", ids.join(", "))
                })?;
            generations.insert((bin.label.to_string(), name.clone()), generated);
            schemas.insert((bin.label.to_string(), name.clone()), validator);
        }
    }
    // The bundle the validator reads must still be the committed one after the generations.
    verify_bundle(&options.bundle)?;
    let crds = options.bundle.join("crds-catalog-cache");
    let kubernetes = OfflineKubernetesValidator::with_crd_catalog(
        &options.bundle.join("kubernetes-json-schema-cache"),
        &crds,
        &options.validate_release,
    )?;

    let mut rows = Vec::new();
    for cell in &cells {
        rows.push(
            adjudicate(
                cell,
                &charts,
                &schemas,
                &bins,
                runner,
                &kubernetes,
                &options.helmsweep,
                &release_minor,
                options.timeout,
            )
            .wrap_err_with(|| format!("cell {}", cell.id))?,
        );
    }
    kubernetes.verify_bundles_unchanged()?;
    verify_bundle(&options.bundle)?;

    let helm = format!(
        "{} sha256={}",
        runner.program_version(),
        runner.program_sha256()
    );
    let manifest = manifest(
        options,
        &cells,
        &base,
        &bins,
        &charts,
        &generations,
        &helm,
        &helmsweep,
    )?;
    let outputs = render_outputs(
        options,
        &cells,
        &rows,
        &bins,
        &charts,
        &helm,
        &helmsweep,
        &release_minor,
        manifest,
    )?;
    publish(options, outputs)
}

/// Writes `outputs`, or under `--check` compares them with the recorded bytes.
fn publish(options: &Options, outputs: Outputs) -> eyre::Result<Outcome> {
    let mut mismatches = outputs.mismatches;
    if options.check {
        for (name, bytes) in &outputs.files {
            let path = options.out.join(name);
            match fs::read(&path) {
                Ok(recorded) if recorded == *bytes => {}
                Ok(_) => mismatches.push(format!(
                    "{} differs from the recomputed bytes",
                    path.display()
                )),
                Err(error) => mismatches.push(format!("{}: {error}", path.display())),
            }
        }
    } else {
        fs::create_dir_all(&options.out)?;
        for (name, bytes) in &outputs.files {
            fs::write(options.out.join(name), bytes)?;
        }
    }
    Ok(if mismatches.is_empty() {
        Outcome::Ok
    } else {
        Outcome::Mismatch(mismatches)
    })
}

/// Every cell's chart, prepared once; under `--check` a chart whose renders are not replayable is
/// refused: it is outside the byte guarantee.
fn prepare_charts(
    options: &Options,
    cells: &[Cell],
    base: &Path,
    runner: &'static HelmRunner,
) -> eyre::Result<BTreeMap<String, PinnedHelmChart>> {
    let mut charts = BTreeMap::new();
    for cell in cells {
        if charts.contains_key(&cell.chart) {
            continue;
        }
        let chart = PinnedHelmChart::prepare_for_explicit_versions(runner, &base.join(&cell.chart))
            .map_err(|error| {
                refuse(format!(
                    "chart {} cannot be prepared: {error:#}",
                    cell.chart
                ))
            })?;
        if options.check
            && let Replay::Bypass(reason) = chart.render_cacheability().replay()
        {
            return Err(refuse(format!(
                "chart {} renders nondeterministically ({reason}): outside --check's guarantee",
                cell.chart
            )));
        }
        charts.insert(cell.chart.clone(), chart);
    }
    Ok(charts)
}

/// `options` with every input path absolute and canonical: generation runs its binary in a scratch
/// directory, where a relative path would name something else.
fn resolved(options: &Options) -> eyre::Result<Options> {
    let canonical = |what: &str, path: &Path| {
        path.canonicalize()
            .map_err(|error| refuse(format!("{what} {}: {error}", path.display())))
    };
    let mut bins = Vec::new();
    for bin in &options.bins {
        bins.push(Bin {
            path: canonical(&format!("--bin {}", bin.label), &bin.path)?,
            ..bin.clone()
        });
    }
    Ok(Options {
        cells: canonical("--cells", &options.cells)?,
        bins,
        helm: canonical("--helm", &options.helm)?,
        helmsweep: canonical("--helmsweep", &options.helmsweep)?,
        bundle: canonical("--bundle", &options.bundle)?,
        gen_k8s_version: options.gen_k8s_version.clone(),
        profile: options.profile.clone(),
        validate_release: options.validate_release.clone(),
        out: options.out.clone(),
        check: options.check,
        require_expected: options.require_expected,
        timeout: options.timeout,
        scratch: options.scratch.clone(),
    })
}

/// Refuses to run unless every scratch directory lies under the cargo target directory named by
/// `CARGO_TARGET_DIR` (the runner rule: no scratch on the system disk's temporary folder), symlinks
/// resolved.
fn check_scratch_root() -> eyre::Result<()> {
    if std::env::var_os("CARGO_TARGET_DIR").is_none_or(|dir| dir.is_empty()) {
        return Err(refuse(
            "CARGO_TARGET_DIR must name the cargo target directory scratch lives in",
        ));
    }
    let target = resolved_path(&test_util::scratch::target_dir())?;
    let root = resolved_path(&test_util::scratch::root())?;
    if !root.starts_with(&target) {
        return Err(refuse(format!(
            "the scratch root {} lies outside the target directory {}",
            root.display(),
            target.display()
        )));
    }
    Ok(())
}

/// Makes the existing directory `dir` the scratch root (`--scratch`), refusing one under `out`.
fn use_scratch(dir: &Path, out: &Path) -> eyre::Result<()> {
    let unusable = || {
        refuse(format!(
            "--scratch {} must be an existing writable directory: cell_matrix never creates it; name one \
             prepared for this run (for example $TMPDIR/cm-check, created with mkdir beforehand)",
            dir.display()
        ))
    };
    let dir = dir
        .canonicalize()
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(unusable)?;
    tempfile::tempfile_in(&dir).map_err(|_| unusable())?;
    if dir.starts_with(resolved_path(out)?) {
        return Err(refuse(format!(
            "--scratch {} lies under --out",
            dir.display()
        )));
    }
    test_util::scratch::use_root(&dir).map_err(|error| refuse(format!("--scratch: {error}")))
}

/// `path` made absolute, its longest existing ancestor resolved through symlinks; a `..`, or an
/// existing entry that cannot be resolved (a dangling symlink), is refused.
fn resolved_path(path: &Path) -> eyre::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    if path
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(refuse(format!("{} climbs with `..`", path.display())));
    }
    let mut existing = path.as_path();
    let mut missing = Vec::new();
    while fs::symlink_metadata(existing).is_err() {
        missing.push(
            existing
                .file_name()
                .ok_or_eyre("a root that does not exist")?,
        );
        existing = existing.parent().ok_or_eyre("a root that does not exist")?;
    }
    let mut resolved = existing
        .canonicalize()
        .map_err(|error| refuse(format!("{}: {error}", existing.display())))?;
    for name in missing.iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

/// Under `--check`, refuses binaries other than the recorded ones before anything runs.
fn check_recorded_bins(options: &Options, bins: &[BinIdentity<'_>]) -> eyre::Result<()> {
    let path = options.out.join("manifest.json");
    let recorded: Value = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| refuse(format!("--check needs the recorded {}", path.display())))?;
    let mut recorded_bins = BTreeMap::new();
    if let Some(bins) = recorded.get("bins").and_then(Value::as_object) {
        for (label, bin) in bins {
            let sha256 = bin
                .get("sha256")
                .and_then(Value::as_str)
                .unwrap_or_default();
            recorded_bins.insert(label.clone(), sha256.to_string());
        }
    }
    let recorded = recorded_bins;
    let given: BTreeMap<String, String> = bins
        .iter()
        .map(|bin| (bin.label.to_string(), bin.sha256.clone()))
        .collect();
    if recorded != given {
        return Err(refuse(format!(
            "--check: the binaries {given:?} are not the recorded {recorded:?}"
        )));
    }
    Ok(())
}

fn read_cells(path: &Path, base: &Path, options: &Options) -> eyre::Result<Vec<Cell>> {
    let text = fs::read_to_string(path)
        .map_err(|error| refuse(format!("read {}: {error}", path.display())))?;
    let mut lines = text.lines();
    if lines.next() != Some("cell_id\tchart\tvalues\tkube_version\texpect") {
        return Err(refuse(
            "cells.tsv needs the header `cell_id chart values kube_version expect` (tab-separated)",
        ));
    }
    let labels: BTreeSet<&str> = options.bins.iter().map(|bin| bin.label.as_str()).collect();
    let mut ids = BTreeSet::new();
    let mut cells = Vec::new();
    for (number, line) in lines.enumerate() {
        let row = number + 2;
        let fields: Vec<&str> = line.split('\t').collect();
        let [id, chart, values, kube_version, expect] = fields.as_slice() else {
            return Err(refuse(format!(
                "cells.tsv:{row}: needs exactly five tab-separated fields"
            )));
        };
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        {
            return Err(refuse(format!("cells.tsv:{row}: malformed cell id {id:?}")));
        }
        if !ids.insert(id.to_string()) {
            return Err(refuse(format!("cells.tsv:{row}: duplicate cell id {id}")));
        }
        if Path::new(chart).is_absolute() || !base.join(chart).join("Chart.yaml").is_file() {
            return Err(refuse(format!(
                "cells.tsv:{row}: {chart:?} is no chart directory relative to the cells file"
            )));
        }
        if !valid_kube_version(kube_version) {
            return Err(refuse(format!(
                "cells.tsv:{row}: bad kube version {kube_version:?}"
            )));
        }
        let values_bytes = match values.strip_prefix('@') {
            Some(file) if !Path::new(file).is_absolute() => fs::read(base.join(file))
                .map_err(|error| refuse(format!("cells.tsv:{row}: values file {file}: {error}")))?,
            Some(file) => {
                return Err(refuse(format!(
                    "cells.tsv:{row}: values file {file} must be relative"
                )));
            }
            None => values.as_bytes().to_vec(),
        };
        match serde_yaml::from_slice::<serde_yaml::Value>(&values_bytes) {
            Ok(serde_yaml::Value::Mapping(_) | serde_yaml::Value::Null) => {}
            _ => {
                return Err(refuse(format!(
                    "cells.tsv:{row}: values must be a YAML mapping"
                )));
            }
        }
        let expect = if *expect == "-" {
            if options.require_expected {
                return Err(refuse(format!(
                    "cells.tsv:{row}: cell {id} carries no expectation"
                )));
            }
            None
        } else {
            let mut pairs = Vec::new();
            for token in expect.split(' ') {
                let Some((column, value)) = token.split_once('=') else {
                    return Err(refuse(format!(
                        "cells.tsv:{row}: malformed expectation {token:?}"
                    )));
                };
                if !labels.contains(column) && !HELM_COLUMNS.contains(&column) {
                    return Err(refuse(format!(
                        "cells.tsv:{row}: unknown column {column:?}"
                    )));
                }
                if !valid_expectation(column, value, &labels) {
                    return Err(refuse(format!(
                        "cells.tsv:{row}: {column} cannot be expected to be {value:?}"
                    )));
                }
                pairs.push((column.to_string(), value.to_string()));
            }
            Some(pairs)
        };
        cells.push(Cell {
            id: id.to_string(),
            chart: chart.to_string(),
            values_bytes,
            kube_version: kube_version.to_string(),
            expect,
        });
    }
    if cells.is_empty() {
        return Err(refuse("cells.tsv holds no cells"));
    }
    cells.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(cells)
}

fn valid_kube_version(version: &str) -> bool {
    let (core, prerelease) = version.split_once('-').unwrap_or((version, "x"));
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        && !prerelease.is_empty()
        && prerelease
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// `unresolved:*` classes and exit codes other than Helm's own are never a claim a
/// red/green comparison may rest on.
fn valid_expectation(column: &str, value: &str, labels: &BTreeSet<&str>) -> bool {
    match column {
        "helm_rc" => matches!(value, "0" | "1"),
        "helm_class" => matches!(value, "pass" | "reject"),
        "k8s" => {
            let (verdict, minor) = value.split_once('@').unwrap_or((value, ""));
            let marked = matches!(verdict, "valid" | "invalid" | "uncertain");
            (minor.is_empty()
                && matches!(
                    verdict,
                    "valid" | "invalid" | "uncertain" | "empty" | "undecodable" | "-"
                ))
                || (marked && is_minor(minor))
        }
        label if labels.contains(label) => matches!(value, "accept" | "reject" | "unavailable"),
        _ => false,
    }
}

/// `1.29`-shaped.
fn is_minor(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

/// The `major.minor` of a cell's version such as `1.29.0` or `1.23.0-rc.1`.
fn minor_of(version: &str) -> String {
    version
        .trim_start_matches('v')
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".")
}

/// The `major.minor` of `release`, a release directory of the bundle's Kubernetes cache
/// (`v1.29.0`, `v1.29.0-standalone-strict`): any other spelling, a path included, is refused.
fn bundle_release(bundle: &Path, what: &str, release: &str) -> eyre::Result<String> {
    let refused = || {
        refuse(format!(
            "{what} {release:?} is not a release directory of the bundle"
        ))
    };
    let unprefixed = release.strip_prefix('v').ok_or_else(refused)?;
    let (core, suffix) = match unprefixed.split_once('-') {
        Some((core, suffix)) => (core, Some(suffix)),
        None => (unprefixed, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let [major, minor, patch] = parts.as_slice() else {
        return Err(refused());
    };
    let numeric = [major, minor, patch]
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    let suffix_ok = suffix.is_none_or(|suffix| {
        suffix.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
    });
    if !numeric || !suffix_ok {
        return Err(refused());
    }
    let cache = bundle.join("kubernetes-json-schema-cache");
    let present = fs::read_dir(&cache)
        .map_err(|error| refuse(format!("bundle {}: {error}", cache.display())))?
        .flatten()
        .any(|source| {
            fs::symlink_metadata(source.path().join(release))
                .is_ok_and(|metadata| metadata.is_dir())
        });
    if !present {
        return Err(refused());
    }
    Ok(format!("{major}.{minor}"))
}

/// Refuses a provider bundle other than the committed one.
fn verify_bundle(bundle: &Path) -> eyre::Result<()> {
    for (tree, expected) in [
        ("kubernetes-json-schema-cache", KUBERNETES_BUNDLE_SHA256),
        ("crds-catalog-cache", CRD_BUNDLE_SHA256),
    ] {
        let actual = tree_sha256(&bundle.join(tree))
            .map_err(|error| refuse(format!("bundle {tree}: {error:#}")))?;
        if actual != expected {
            return Err(refuse(format!(
                "bundle {tree} is {actual}, not the committed {expected}"
            )));
        }
    }
    Ok(())
}

/// A bin's identity: its bytes and the commit it is labelled with (`helm-schema` has no
/// `--version`).
struct BinIdentity<'a> {
    bin: &'a Bin,
    label: &'a str,
    sha256: String,
}

fn identify_bins(options: &Options) -> eyre::Result<Vec<BinIdentity<'_>>> {
    let mut labels = BTreeSet::new();
    let mut bins = Vec::new();
    for bin in &options.bins {
        if !labels.insert(bin.label.as_str()) {
            return Err(refuse(format!("--bin label {} repeats", bin.label)));
        }
        if HELM_COLUMNS.contains(&bin.label.as_str())
            || [
                "cell_id",
                "prep_sha256",
                "values_src",
                "helm_abort",
                "expect_ok",
            ]
            .contains(&bin.label.as_str())
        {
            return Err(refuse(format!(
                "--bin label {} names another column",
                bin.label
            )));
        }
        if bin.commit.len() != 40 || !bin.commit.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(refuse(format!(
                "--bin {}: commit {:?} is not 40 hex",
                bin.label, bin.commit
            )));
        }
        let bytes = fs::read(&bin.path).map_err(|error| {
            refuse(format!(
                "--bin {}: {}: {error}",
                bin.label,
                bin.path.display()
            ))
        })?;
        bins.push(BinIdentity {
            bin,
            label: &bin.label,
            sha256: sha256_hex(&bytes),
        });
    }
    if bins.is_empty() {
        return Err(refuse("give at least one --bin LABEL=PATH@COMMIT"));
    }
    Ok(bins)
}

/// `program arguments`'s trimmed stdout under an empty environment.
fn program_identity(program: &Path, arguments: &[&str], timeout: Duration) -> eyre::Result<String> {
    let what = format!("{} {}", program.display(), arguments.join(" "));
    let output = output_within(
        Command::new(program).env_clear().args(arguments),
        timeout,
        &what,
    )?;
    if !output.status.success() {
        return Err(refuse(format!("{what}: exit {}", output.status)));
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// The Helm runner of `helm`, the CLI engine only: no ambient engine selection applies.
/// The replay store is `SCHEMA_HELM_INVOCATION_CACHE`, except under `--scratch` (`private`), where every
/// write stays in the private scratch and nothing is replayed.
fn helm_runner(helm: &Path, timeout: Duration, private: bool) -> eyre::Result<&'static HelmRunner> {
    let ambient = std::env::var_os("SCHEMA_HELM_INVOCATION_CACHE").filter(|_| !private);
    let (root, replay) = match ambient {
        Some(root) => (PathBuf::from(root).join("v2"), true),
        None => (ScratchDir::new("cell-matrix-helm")?.keep(), false),
    };
    // A hung identity probe is a harness failure (exit 3); another Helm is refused (exit 2).
    let runner = HelmRunner::with_program_within(&root, replay, helm.to_path_buf(), timeout)
        .map_err(|error| {
            if error.downcast_ref::<TimedOut>().is_some() {
                error
            } else {
                refuse(format!("--helm: {error:#}"))
            }
        })?;
    Ok(Box::leak(Box::new(runner)))
}

/// The arguments of one generation, with the chart, bundle and output as placeholders.
fn recipe(options: &Options) -> Vec<String> {
    [
        "<chart>",
        "--no-config",
        "--k8s-version",
        &options.gen_k8s_version,
        "--strict-k8s-version",
        "--offline",
        "--k8s-schema-cache-dir",
        "<bundle>/kubernetes-json-schema-cache",
        "--crd-catalog-cache-dir",
        "<bundle>/crds-catalog-cache",
        "--profile",
        &options.profile,
        "--exclude-tests",
        "--output",
        "<out>",
    ]
    .map(str::to_string)
    .to_vec()
}

/// One generated schema's identity: its bytes and the draft the prober compiles it under.
struct Generated {
    sha256: String,
    draft: String,
}

/// Generates `bin`'s schema of the prepared `chart` and compiles it.
fn generate(
    options: &Options,
    bin: &BinIdentity<'_>,
    chart: &PinnedHelmChart,
    scratch: &Path,
) -> eyre::Result<(jsonschema::Validator, Generated)> {
    let case = tempfile::Builder::new()
        .prefix("generate-")
        .tempdir_in(scratch)?
        .keep();
    let out = case.join("schema.json");
    for dir in ["home", "config", "cache"] {
        fs::create_dir_all(case.join(dir))?;
    }
    let chart_path = chart.render_tree().path().to_string_lossy().to_string();
    let bundle = options.bundle.to_string_lossy().to_string();
    let arguments: Vec<String> = recipe(options)
        .into_iter()
        .map(|argument| {
            argument
                .replace("<chart>", &chart_path)
                .replace("<bundle>", &bundle)
                .replace("<out>", &out.to_string_lossy())
        })
        .collect();
    let output = output_within(
        Command::new(&bin.bin.path)
            .env_clear()
            .env("HOME", case.join("home"))
            .env("XDG_CONFIG_HOME", case.join("config"))
            .env("XDG_CACHE_HOME", case.join("cache"))
            .current_dir(&case)
            .args(&arguments),
        options.timeout,
        &format!("--bin {}", bin.label),
    )?;
    eyre::ensure!(
        output.status.success(),
        "--bin {} failed ({}): {}",
        bin.label,
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = fs::read(&out).wrap_err_with(|| format!("--bin {} wrote no schema", bin.label))?;
    let schema: Value = serde_json::from_slice(&bytes)
        .wrap_err_with(|| format!("--bin {}: the schema is not JSON", bin.label))?;
    let validator = jsonschema::validator_for(&schema).map_err(|error| {
        eyre::eyre!("--bin {}: the schema does not compile: {error}", bin.label)
    })?;
    let draft = format!("{:?}", jsonschema::Draft::default().detect(&schema));
    Ok((
        validator,
        Generated {
            sha256: sha256_hex(&bytes),
            draft,
        },
    ))
}

/// One cell's measured columns.
#[derive(Clone)]
struct Row {
    values_src: &'static str,
    verdicts: Vec<(String, &'static str)>,
    helm_rc: i32,
    helm_class: String,
    helm_abort: String,
    k8s: String,
    diagnostics: Vec<[String; 5]>,
}

#[allow(
    clippy::too_many_arguments,
    reason = "one cell needs every prepared input of the run"
)]
fn adjudicate(
    cell: &Cell,
    charts: &BTreeMap<String, PinnedHelmChart>,
    schemas: &BTreeMap<(String, String), jsonschema::Validator>,
    bins: &[BinIdentity<'_>],
    runner: &HelmRunner,
    kubernetes: &OfflineKubernetesValidator,
    helmsweep: &Path,
    release_minor: &str,
    timeout: Duration,
) -> eyre::Result<Row> {
    let chart = charts.get(&cell.chart).ok_or_eyre("unprepared chart")?;
    let probe = chart.adjudicate_file(&cell.values_bytes, &cell.kube_version)?;
    let mut diagnostics = Vec::new();
    let (instance, values_src) = match probe.values {
        Some(values) => (Some(values.as_json().clone()), "helm"),
        None => match rust_coalesced(chart, &cell.values_bytes, &probe.evidence_dir)? {
            Ok(instance) => (Some(instance), "rust"),
            Err(reason) => {
                diagnostics.push([
                    "values".to_string(),
                    String::new(),
                    "unavailable".to_string(),
                    normalize(&reason, chart, runner, &probe.evidence_dir),
                    String::new(),
                ]);
                (None, "-")
            }
        },
    };
    let mut verdicts = Vec::new();
    for bin in bins {
        let validator = schemas
            .get(&(bin.label.to_string(), cell.chart.clone()))
            .ok_or_eyre("ungenerated schema")?;
        let verdict = match &instance {
            None => "unavailable",
            Some(instance) => {
                let keys: BTreeSet<ViolationKey> = validator
                    .iter_errors(instance)
                    .map(|error| ViolationKey::new("", "", &error))
                    .collect();
                for key in &keys {
                    diagnostics.push([
                        bin.label.to_string(),
                        key.instance_path.clone(),
                        key.keyword.clone(),
                        key.detail.clone(),
                        key.schema_path.clone(),
                    ]);
                }
                if keys.is_empty() { "accept" } else { "reject" }
            }
        };
        verdicts.push((bin.label.to_string(), verdict));
    }
    let rendered = &probe.rendered;
    let log = probe.evidence_dir.join("render.classify.log");
    fs::write(&log, &rendered.stderr)?;
    let classified = output_within(
        Command::new(helmsweep)
            .env_clear()
            .args(["classify", "template", &rendered.exit_code.to_string()])
            .arg(&log),
        timeout,
        &format!("{} classify", helmsweep.display()),
    )?;
    eyre::ensure!(
        classified.status.success(),
        "helmsweep classify failed: {}",
        classified.status
    );
    let helm_class = String::from_utf8(classified.stdout)?.trim().to_string();
    let helm_abort = String::from_utf8_lossy(&rendered.stderr)
        .lines()
        .find(|line| line.starts_with("Error:"))
        .map_or_else(
            || "-".to_string(),
            |line| normalize(line, chart, runner, &probe.evidence_dir),
        );
    let mut k8s = k8s_column(
        rendered,
        &probe.evidence_dir,
        chart,
        runner,
        kubernetes,
        &mut diagnostics,
    )?
    .to_string();
    if matches!(k8s.as_str(), "valid" | "invalid" | "uncertain")
        && minor_of(&cell.kube_version) != release_minor
    {
        k8s = format!("{k8s}@{release_minor}");
    }
    diagnostics.sort();
    diagnostics.dedup();
    Ok(Row {
        values_src,
        verdicts,
        helm_rc: rendered.exit_code,
        helm_class,
        helm_abort,
        k8s,
        diagnostics,
    })
}

/// The `k8s` column of a render, its violations and uncertainties added to `diagnostics`.
fn k8s_column(
    rendered: &HelmExecution,
    case: &Path,
    chart: &PinnedHelmChart,
    runner: &HelmRunner,
    kubernetes: &OfflineKubernetesValidator,
    diagnostics: &mut Vec<[String; 5]>,
) -> eyre::Result<&'static str> {
    if !rendered.success() {
        return Ok("-");
    }
    Ok(match decode(runner, &rendered.stdout, case)?.documents {
        Err(rejection) => {
            diagnostics.push([
                "k8s".to_string(),
                String::new(),
                "undecodable".to_string(),
                normalize(&rejection, chart, runner, case),
                String::new(),
            ]);
            "undecodable"
        }
        // No resource document: nothing but whitespace, separators or comments.
        Ok(documents) if documents.iter().all(Value::is_null) => "empty",
        Ok(documents) => match kubernetes.validate_documents(&documents) {
            KubernetesVerdict::Valid => "valid",
            KubernetesVerdict::Invalid(violations) => {
                for violation in violations {
                    diagnostics.push([
                        "k8s".to_string(),
                        format!(
                            "{}/{} {}",
                            violation.key.api_version,
                            violation.key.kind,
                            violation.key.instance_path
                        ),
                        violation.key.keyword.clone(),
                        violation.key.detail.clone(),
                        violation.key.schema_path.clone(),
                    ]);
                }
                "invalid"
            }
            KubernetesVerdict::Uncertain(reasons) => {
                for reason in reasons {
                    diagnostics.push([
                        "k8s".to_string(),
                        String::new(),
                        "uncertain".to_string(),
                        reason,
                        String::new(),
                    ]);
                }
                "uncertain"
            }
        },
    })
}

/// The Rust port's coalesced document when Helm aborted coalescing, or why there is none.
fn rust_coalesced(
    chart: &PinnedHelmChart,
    values: &[u8],
    case: &Path,
) -> eyre::Result<Result<Value, String>> {
    let file = case.join("override.values.yaml");
    fs::write(&file, values)?;
    let options = ValuesOptions {
        value_files: vec![file],
        ..ValuesOptions::default()
    };
    let composed = options
        .merge_values()
        .and_then(|overrides| coalesce_chart_values(chart.render_tree().path(), overrides));
    match composed {
        Ok(instance) => Ok(Ok(instance)),
        Err(ValuesError::NotValidated(reason) | ValuesError::Unmodelled(reason)) => Ok(Err(reason)),
        Err(error) => Err(error.into()),
    }
}

/// `text` with the run's scratch locations replaced by stable names.
fn normalize(text: &str, chart: &PinnedHelmChart, runner: &HelmRunner, case: &Path) -> String {
    text.replace(&case.to_string_lossy().to_string(), "<case>")
        .replace(
            &chart.render_tree().path().to_string_lossy().to_string(),
            "<chart>",
        )
        .replace(&runner.root().to_string_lossy().to_string(), "<root>")
        .replace(['\t', '\n', '\r'], " ")
}

/// matrix.tsv and diagnostics.tsv beside `manifest`, and the expectations that did not hold.
#[allow(
    clippy::too_many_arguments,
    reason = "the table header names every input of the run"
)]
fn render_outputs(
    options: &Options,
    cells: &[Cell],
    rows: &[Row],
    bins: &[BinIdentity<'_>],
    charts: &BTreeMap<String, PinnedHelmChart>,
    helm: &str,
    helmsweep: &str,
    release_minor: &str,
    manifest: Vec<u8>,
) -> eyre::Result<Outputs> {
    let mut table = String::new();
    writeln!(table, "# {FORMAT}")?;
    writeln!(table, "# helm: {helm}")?;
    writeln!(
        table,
        "# helmsweep: {}",
        helmsweep.lines().next().unwrap_or("")
    )?;
    writeln!(
        table,
        "# bundle: kubernetes={KUBERNETES_BUNDLE_SHA256} crds={CRD_BUNDLE_SHA256} validate={}",
        options.validate_release
    )?;
    writeln!(table, "# generation: {}", recipe(options).join(" "))?;
    writeln!(
        table,
        "# versions: helm renders each cell at its kube_version; every schema is generated at {}; k8s \
         validates against {} (a verdict for a cell at another minor is marked @{})",
        options.gen_k8s_version, options.validate_release, release_minor
    )?;
    for bin in bins {
        writeln!(
            table,
            "# bin {}: commit={} sha256={}",
            bin.label, bin.bin.commit, bin.sha256
        )?;
    }
    let labels: Vec<&str> = bins.iter().map(|bin| bin.label).collect();
    writeln!(
        table,
        "cell_id\tprep_sha256\tvalues_src\t{}\thelm_rc\thelm_class\thelm_abort\tk8s\texpect_ok",
        labels.join("\t")
    )?;
    let mut diagnostics =
        String::from("cell_id\tcolumn\tinstance_ptr\tkeyword\tdetail\tschema_ptr\n");
    let mut mismatches = Vec::new();
    for (cell, row) in cells.iter().zip(rows) {
        let chart = charts.get(&cell.chart).ok_or_eyre("unprepared chart")?;
        let expect_ok = match &cell.expect {
            None => "-",
            Some(pairs) => {
                let mut ok = true;
                for (column, want) in pairs {
                    let have = observed(row, column);
                    if have != *want {
                        ok = false;
                        mismatches
                            .push(format!("({}, {column}, {want}): observed {have}", cell.id));
                    }
                }
                if ok { "yes" } else { "NO" }
            }
        };
        let verdicts: Vec<&str> = row.verdicts.iter().map(|(_, verdict)| *verdict).collect();
        writeln!(
            table,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{expect_ok}",
            cell.id,
            chart.render_tree().sha256(),
            row.values_src,
            verdicts.join("\t"),
            row.helm_rc,
            row.helm_class,
            row.helm_abort,
            row.k8s,
        )?;
        for [column, pointer, keyword, detail, schema] in &row.diagnostics {
            writeln!(
                diagnostics,
                "{}\t{column}\t{pointer}\t{keyword}\t{}\t{schema}",
                cell.id,
                detail.replace(['\t', '\n', '\r'], " ")
            )?;
        }
    }
    Ok(Outputs {
        files: vec![
            ("matrix.tsv", table.into_bytes()),
            ("diagnostics.tsv", diagnostics.into_bytes()),
            ("manifest.json", manifest),
        ],
        mismatches,
    })
}

/// manifest.json: every input's identity, path-free.
#[allow(
    clippy::too_many_arguments,
    reason = "the manifest names every input of the run"
)]
fn manifest(
    options: &Options,
    cells: &[Cell],
    base: &Path,
    bins: &[BinIdentity<'_>],
    charts: &BTreeMap<String, PinnedHelmChart>,
    generations: &BTreeMap<(String, String), Generated>,
    helm: &str,
    helmsweep: &str,
) -> eyre::Result<Vec<u8>> {
    let mut sources = BTreeMap::new();
    for name in charts.keys() {
        sources.insert(name.clone(), tree_sha256(&base.join(name))?);
    }
    let manifest = json!({
        "format": FORMAT,
        "cells": cells.iter().map(|cell| json!({
            "id": cell.id,
            "chart": cell.chart,
            "values_sha256": sha256_hex(&cell.values_bytes),
            "kube_version": cell.kube_version,
        })).collect::<Vec<_>>(),
        "charts": charts.iter().map(|(name, chart)| (name.clone(), json!({
            "source_sha256": sources.get(name),
            "prepared_sha256": chart.render_tree().sha256(),
            "coalesce_sha256": chart.coalesce_tree().sha256(),
            "preparation": "helm-schema/prepared-chart/v1: shipped values.schema.json and templates/tests removed; --skip-schema-validation",
        }))).collect::<BTreeMap<_, _>>(),
        "bins": bins.iter().map(|bin| (bin.label.to_string(), json!({
            "commit": bin.bin.commit,
            "sha256": bin.sha256,
            "schemas": generations.iter()
                .filter(|((label, _), _)| label == bin.label)
                .map(|((_, chart), generated)| (chart.clone(), json!({
                    "sha256": generated.sha256,
                    "draft": generated.draft,
                })))
                .collect::<BTreeMap<_, _>>(),
        }))).collect::<BTreeMap<_, _>>(),
        "generation": recipe(options),
        "generation_settings": {
            "k8s_version": options.gen_k8s_version,
            "strict_k8s_version": true,
            "offline": true,
            "no_config": true,
            "exclude_tests": true,
            "profile": options.profile,
            "k8s_schema_cache": "<bundle>/kubernetes-json-schema-cache",
            "crd_catalog_cache": "<bundle>/crds-catalog-cache",
            "environment": "empty, with HOME and XDG dirs in scratch",
        },
        "helm": helm,
        "helmsweep": {
            "version": helmsweep,
            "sha256": sha256_hex(&fs::read(&options.helmsweep)?),
        },
        "bundle": {
            "kubernetes_sha256": KUBERNETES_BUNDLE_SHA256,
            "crds_sha256": CRD_BUNDLE_SHA256,
            "validate_release": options.validate_release,
        },
        "prober": {
            "crate": "jsonschema",
            "version": env!("HELM_SCHEMA_JSONSCHEMA_VERSION"),
            "draft": "detected from each schema's $schema (per schema above)",
            "violation_key": "instance path, schema path, keyword, detail",
        },
        "replay": "helm-schema/helm-invocation/v3",
    });
    let mut manifest = serde_json::to_vec_pretty(&manifest)?;
    manifest.push(b'\n');
    Ok(manifest)
}

fn observed(row: &Row, column: &str) -> String {
    match column {
        "helm_rc" => row.helm_rc.to_string(),
        "helm_class" => row.helm_class.clone(),
        "k8s" => row.k8s.clone(),
        label => row
            .verdicts
            .iter()
            .find(|(name, _)| name == label)
            .map_or_else(String::new, |(_, verdict)| (*verdict).to_string()),
    }
}
