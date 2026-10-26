//! Dependency and imported-context screening against Helm's three schema documents.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::{helm::invocation::tree_sha256, manifest, source_digest::sha256_hex};
use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use test_util::helm_values::{
    AcceptanceDocument, AcceptanceInputs, DependencyProbeKind, ValuesError, ValuesOptions,
};
use test_util::scratch::ScratchDir;

use super::discovery::{
    HelmChartDir, MAX_PROBES_PER_CHART, ProbeCoverage, ProbeInstance, ProbeValuesFile,
    ProfileSchemas, UnreachableProbe, structural_probe_battery_with_coverage,
};
use super::{
    ChartCost, ChartHelm, ComparedChart, LiveChart, LoadedInputs, ScreenedChart, ScreenedWitness,
    WitnessSink, elapsed_ms,
};

/// Names the imported context roster file in the ordinary battery.
pub const CONTEXT_ROSTER_VAR: &str = "SCHEMA_ACCEPTANCE_CONTEXT_ROSTER";
const MAX_OVERLAYS: usize = 8_192;
const MAX_ROSTER_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// One imported context row: a tested overlay and its context-only overlay.
pub struct ContextRow {
    /// The row's identity within its chart.
    pub id: String,
    /// The chart's label.
    pub chart: String,
    /// The tested overlay.
    pub overlay: Value,
    /// How it reaches Helm; only `values-file`.
    pub transport: String,
    /// The Kubernetes version to render for.
    pub kubernetes_version: String,
    /// SHA-256 of the chart tree the row was recorded for.
    pub chart_sha256: String,
    /// SHA-256 of the installed baseline schema.
    pub baseline_sha256: String,
    /// SHA-256 of the installed candidate schema.
    pub candidate_sha256: String,
    /// SHA-256 of the canonical tested overlay.
    pub canonical_overlay_sha256: String,
    /// The original values-file text.
    pub values_file: String,
    /// SHA-256 of that text.
    pub values_file_sha256: String,
    /// The context-only overlay.
    pub context_overlay: Value,
    /// SHA-256 of the canonical context-only overlay.
    pub context_overlay_sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// A version-2 context roster file.
pub struct ContextRoster {
    /// The format version; only 2.
    pub version: u32,
    /// Its rows.
    pub rows: Vec<ContextRow>,
}

/// A verified context roster, grouped by chart.
pub struct ImportedRoster {
    /// SHA-256 of the reviewed roster file's exact bytes.
    pub sha256: String,
    /// Each chart's rows, sorted by identity.
    pub rows: BTreeMap<String, Vec<ContextRow>>,
    /// Whether each row's recorded chart and schema hashes are rebound to the
    /// pair being compared, as a comparison history's one reviewed roster
    /// must be across checkpoints.
    pub rebind: bool,
}

impl ImportedRoster {
    /// Reads and verifies the roster at `path` for exactly the selected `charts`.
    ///
    /// # Errors
    ///
    /// Returns an error when the file is too large, malformed, or a row is inconsistent or names an unselected chart.
    pub fn read(path: &Path, charts: &BTreeSet<String>) -> eyre::Result<Self> {
        Self::parse(path, charts, false)
    }

    /// [`ImportedRoster::read`] for a comparison history: rows are rebound to
    /// each compared pair; its exact reviewed bytes remain its identity.
    ///
    /// # Errors
    ///
    /// As [`ImportedRoster::read`].
    pub fn read_rebinding(path: &Path, charts: &BTreeSet<String>) -> eyre::Result<Self> {
        Self::parse(path, charts, true)
    }

    fn parse(path: &Path, charts: &BTreeSet<String>, rebind: bool) -> eyre::Result<Self> {
        eyre::ensure!(
            std::fs::metadata(path)?.len() <= MAX_ROSTER_BYTES,
            "context roster exceeds byte cap"
        );
        let bytes = std::fs::read(path)?;
        let roster: ContextRoster = serde_json::from_slice(&bytes)?;
        eyre::ensure!(
            roster.version == 2,
            "unsupported context roster version {}",
            roster.version
        );
        eyre::ensure!(!roster.rows.is_empty(), "empty context roster");
        let scratch = ScratchDir::new("context-roster")?;
        let mut rows = BTreeMap::<String, Vec<ContextRow>>::new();
        let mut identities = BTreeSet::new();
        for row in roster.rows {
            eyre::ensure!(
                charts.contains(&row.chart),
                "context row {} names unselected chart {}",
                row.id,
                row.chart
            );
            eyre::ensure!(
                !row.id.is_empty() && identities.insert((row.chart.clone(), row.id.clone())),
                "empty or duplicate context row id {}",
                row.id
            );
            eyre::ensure!(
                row.overlay.is_object(),
                "context row {} overlay is not an object",
                row.id
            );
            eyre::ensure!(
                row.transport == "values-file",
                "unsupported context transport {}",
                row.transport
            );
            eyre::ensure!(
                !row.kubernetes_version.is_empty(),
                "context row {} has no Kubernetes version",
                row.id
            );
            eyre::ensure!(
                sha256_hex(&serde_json::to_vec(&row.overlay)?) == row.canonical_overlay_sha256,
                "context row {} overlay hash mismatch",
                row.id
            );
            eyre::ensure!(
                sha256_hex(row.values_file.as_bytes()) == row.values_file_sha256,
                "context row {} original values-file hash mismatch",
                row.id
            );
            let values_file = tempfile::NamedTempFile::new_in(scratch.path())?;
            std::fs::write(values_file.path(), row.values_file.as_bytes())?;
            let parsed = ValuesOptions {
                value_files: vec![values_file.path().to_owned()],
                ..ValuesOptions::default()
            }
            .merge_values()?;
            eyre::ensure!(
                parsed == row.overlay,
                "context row {} original values file differs from overlay",
                row.id
            );
            eyre::ensure!(
                row.context_overlay.is_object()
                    && sha256_hex(&serde_json::to_vec(&row.context_overlay)?)
                        == row.context_overlay_sha256,
                "context row {} context overlay or hash mismatch",
                row.id
            );
            let chart_rows = rows.entry(row.chart.clone()).or_default();
            eyre::ensure!(
                chart_rows.len() < MAX_OVERLAYS,
                "context rows exceed per-chart cap"
            );
            chart_rows.push(row);
        }
        for chart_rows in rows.values_mut() {
            chart_rows.sort_by(|left, right| left.id.cmp(&right.id));
        }
        Ok(Self {
            sha256: sha256_hex(&bytes),
            rows,
            rebind,
        })
    }
}

/// The exact root schemas installed for lint, derived without rerunning inference.
pub struct LintSchemas {
    /// The installed baseline schema bytes.
    pub baseline: Vec<u8>,
    /// The installed candidate schema bytes.
    pub candidate: Vec<u8>,
    /// Both schemas compiled for screening.
    pub profiles: ProfileSchemas,
    /// SHA-256 of the candidate's producer manifest.
    pub manifest_sha256: Option<String>,
}

impl LintSchemas {
    /// Compiles `baseline` and `candidate` as installed for lint.
    ///
    /// # Errors
    ///
    /// Returns an error when a schema cannot be serialized, compiled or verified.
    pub fn compile(
        baseline: &Value,
        candidate: &Value,
        candidate_path: Option<&Path>,
    ) -> eyre::Result<Self> {
        Self::compile_bytes(&serde_json::to_vec(baseline)?, candidate, candidate_path)
    }

    /// Compiles the shipped `baseline` bytes and `candidate`, verifying the candidate's companion when it comes from a dump.
    ///
    /// # Errors
    ///
    /// Returns an error when a schema cannot be parsed, compiled or verified against its manifest.
    pub fn compile_bytes(
        baseline: &[u8],
        candidate: &Value,
        candidate_path: Option<&Path>,
    ) -> eyre::Result<Self> {
        // The sweep ships committed bytes unchanged unless Helm's per-file limit
        // requires shortening.
        let baseline = if baseline.len() <= helm_schema::output::HELM_MAX_CHART_FILE_BYTES {
            baseline.to_vec()
        } else {
            helm_ready(&serde_json::from_slice(baseline)?)?
        };
        let candidate = if let Some(path) = candidate_path {
            verified_companion(path, candidate)?
        } else {
            helm_ready(candidate)?
        };
        // Regex respelling is validator policy, never part of the shipped bytes or their identity.
        let mut validation_baseline = serde_json::from_slice(&baseline)?;
        helm_schema_core::normalize_schema_pattern_dialects(&mut validation_baseline);
        let profiles = ProfileSchemas::compile(
            &validation_baseline,
            &serde_json::from_slice(&candidate)?,
            json!({}),
        )?;
        let manifest_sha256 = candidate_path
            .map(|path| {
                let directory = path
                    .parent()
                    .ok_or_eyre("candidate has no dump directory")?;
                Ok::<_, eyre::Report>(sha256_hex(&std::fs::read(
                    directory.join(manifest::MANIFEST_FILE),
                )?))
            })
            .transpose()?;
        Ok(Self {
            baseline,
            candidate,
            profiles,
            manifest_sha256,
        })
    }
}

/// `schema` with shortened definition names, as Helm reads it.
///
/// # Errors
///
/// Returns an error when the schema cannot be serialized.
pub fn helm_ready(schema: &Value) -> eyre::Result<Vec<u8>> {
    let shortened = helm_schema_json_schema_minify::shorten_definition_names(schema);
    let mut bytes = serde_json::to_vec(&shortened.schema)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn verified_companion(path: &Path, candidate: &Value) -> eyre::Result<Vec<u8>> {
    let directory = path
        .parent()
        .ok_or_eyre("candidate has no dump directory")?;
    let bytes = std::fs::read(path)?;
    let manifest_bytes = std::fs::read(directory.join(manifest::MANIFEST_FILE))
        .wrap_err("lint lane needs the candidate producer manifest")?;
    let manifest: manifest::Manifest = serde_json::from_slice(&manifest_bytes)?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_eyre("candidate filename is not UTF-8")?;
    let mut entries = manifest
        .artifacts
        .iter()
        .filter(|entry| entry.file == filename);
    let entry = entries
        .next()
        .ok_or_eyre("candidate absent from producer manifest")?;
    eyre::ensure!(
        entries.next().is_none(),
        "duplicate candidate manifest entry"
    );
    eyre::ensure!(
        sha256_hex(&bytes) == entry.sha256 && u64::try_from(bytes.len())? == entry.size,
        "candidate differs from producer manifest"
    );
    // Lean controls have no shipped companion; their equivalent spelling is still exact.
    let Some(companion) = &entry.helm_ready else {
        eyre::ensure!(
            entry.key.starts_with("lean/"),
            "candidate has no Helm-ready companion; produce with --helm-ready"
        );
        return helm_ready(candidate);
    };
    let relative = PathBuf::from(&companion.schema.file);
    eyre::ensure!(
        relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_))),
        "invalid companion path"
    );
    let shipped = std::fs::read(directory.join(relative))?;
    eyre::ensure!(
        sha256_hex(&shipped) == companion.schema.sha256
            && u64::try_from(shipped.len())? == companion.schema.size,
        "Helm-ready companion hash mismatch"
    );
    eyre::ensure!(
        shipped == helm_ready(candidate)?,
        "Helm-ready companion differs from candidate"
    );
    Ok(shipped)
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
/// One document kind's accounting in a chart's lint lane.
pub struct DocumentCoverage {
    /// Documents composed.
    pub composed: usize,
    /// Documents Helm would not validate.
    pub not_validated: usize,
    /// Documents that failed to compose.
    pub errors: usize,
    /// Documents screened.
    pub screened: usize,
    /// Documents screened as flips.
    pub flips: usize,
    /// Flips adjudicated.
    pub adjudicated: usize,
    /// Flips whose adjudication failed.
    pub adjudication_failures: usize,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
/// A chart's lint lane accounting and its input identities.
pub struct LintCoverage {
    /// Built-in dependency rows.
    pub builtin_rows: usize,
    /// Imported context rows.
    pub imported_rows: usize,
    /// Row origins screened.
    pub rows_screened: usize,
    /// Distinct values-file inputs.
    pub distinct_overlays: usize,
    /// Each document kind's accounting.
    pub documents: BTreeMap<String, DocumentCoverage>,
    /// SHA-256 of the imported roster.
    pub roster_sha256: Option<String>,
    /// SHA-256 of the chart tree.
    pub chart_sha256: String,
    /// SHA-256 of the installed baseline schema.
    pub baseline_sha256: String,
    /// SHA-256 of the installed candidate schema.
    pub candidate_sha256: String,
    /// SHA-256 of the candidate's producer manifest.
    pub candidate_manifest_sha256: Option<String>,
    /// Why the lane is incomplete.
    pub failures: Vec<String>,
    /// Documents Helm would not validate, and why.
    pub inapplicable: Vec<String>,
    /// Each distinct input's evidence.
    pub rows: Vec<RowEvidence>,
    /// Forced raw-lint audits both schemas judge alike: no transition, so a
    /// schema-only run defers them to live adjudication instead of flipping.
    #[serde(default)]
    pub deferred_live_audits: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
/// One distinct values-file input and its origins.
pub struct RowEvidence {
    /// Every row origin it serves.
    pub origins: Vec<String>,
    /// SHA-256 of its canonical overlay.
    pub canonical_overlay_sha256: String,
    /// SHA-256 of its exact values-file bytes.
    pub values_file_sha256: String,
    /// The Kubernetes version to render for.
    pub kubernetes_version: String,
    /// Its context controls.
    pub contexts: Vec<ContextEvidence>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
/// One context control of an input.
pub struct ContextEvidence {
    /// The row origin it came from.
    pub origin: String,
    /// The context-only overlay.
    pub overlay: Value,
    /// SHA-256 of the canonical overlay.
    pub canonical_overlay_sha256: String,
}

impl LintCoverage {
    /// Checks that every row and document is accounted for.
    ///
    /// # Errors
    ///
    /// Returns the first incomplete row or document count.
    pub fn validate(&self) -> eyre::Result<()> {
        eyre::ensure!(
            self.failures.is_empty(),
            "incomplete lint lane: {:?}",
            self.failures
        );
        eyre::ensure!(
            self.builtin_rows > 0 && self.distinct_overlays > 0,
            "empty lint lane"
        );
        eyre::ensure!(
            self.rows_screened == self.builtin_rows + self.imported_rows,
            "lint row coverage mismatch"
        );
        eyre::ensure!(
            self.rows.len() == self.distinct_overlays,
            "lint overlay evidence mismatch"
        );
        for kind in AcceptanceDocument::ALL {
            let counts = self
                .documents
                .get(&format!("{kind:?}"))
                .ok_or_eyre("missing document coverage")?;
            eyre::ensure!(
                counts.errors == 0
                    && counts.composed == counts.screened
                    && counts.composed + counts.not_validated == self.distinct_overlays,
                "incomplete {kind:?} coverage: {counts:?}"
            );
            eyre::ensure!(
                counts.screened > 0
                    || (matches!(
                        kind,
                        AcceptanceDocument::LintRaw | AcceptanceDocument::LintCoalescedTwice
                    ) && counts.not_validated == self.distinct_overlays),
                "no {kind:?} documents screened"
            );
        }
        Ok(())
    }

    /// Checks [`Self::validate`] and that every flip was adjudicated.
    ///
    /// # Errors
    ///
    /// Returns the first incomplete count.
    pub fn validate_live(&self) -> eyre::Result<()> {
        self.validate()?;
        for (kind, counts) in &self.documents {
            eyre::ensure!(
                counts.flips == counts.adjudicated + counts.adjudication_failures,
                "incomplete {kind} adjudication: {counts:?}"
            );
        }
        Ok(())
    }

    /// Documents screened across every kind.
    #[must_use]
    pub fn probes(&self) -> usize {
        self.documents.values().map(|counts| counts.screened).sum()
    }
}

#[derive(Clone)]
/// One screened flip handed to Helm.
pub struct ScreenedProbe {
    /// The probe's name.
    pub name: String,
    /// The overlay Helm receives.
    pub overlay: Value,
    /// `None` retains the legacy exact-template adjudication path.
    pub document: Option<ScreenedDocument>,
}

#[derive(Clone)]
/// A screened document of the lint lane.
pub struct ScreenedDocument {
    /// Which of Helm's documents it is.
    pub kind: AcceptanceDocument,
    /// Audit a rejected raw lint control even when both schemas reject it alike.
    pub lint_control: bool,
    /// The composed document.
    pub values: Value,
    /// The same kind of document for the chart's defaults.
    pub defaults: Option<Value>,
    /// The Kubernetes version to render for.
    pub kubernetes_version: String,
    /// Every row origin it serves.
    pub origins: Vec<String>,
    /// The exact values-file bytes.
    pub values_file: Vec<u8>,
    /// Applicable context-only overlays, defaults included.
    pub control_overlays: Vec<Value>,
}

struct OverlayRow {
    overlay: Value,
    values_file: Vec<u8>,
    kubernetes_version: String,
    origins: Vec<String>,
    contexts: Vec<ContextEvidence>,
    lint_control: bool,
}

/// A chart's lint lane screening.
pub struct LintScreen {
    /// Its accounting.
    pub coverage: LintCoverage,
    /// The documents screened as flips.
    pub flips: Vec<ScreenedProbe>,
}

/// What the lint lane screens.
pub struct ScreenRequest<'a> {
    /// The staged chart.
    pub chart: &'a Path,
    /// Both schemas as installed for lint.
    pub schemas: &'a LintSchemas,
    /// The chart's Kubernetes version.
    pub kubernetes_version: &'a str,
    /// The chart's imported context rows.
    pub imported: &'a [ContextRow],
    /// SHA-256 of the imported roster.
    pub roster_sha256: Option<String>,
    /// Documents the chart may still screen.
    pub available_probes: usize,
}

/// Screens the chart's dependency and context documents.
///
/// # Errors
///
/// Returns an error when the chart's inputs cannot be loaded or composed.
pub fn screen(request: &ScreenRequest<'_>) -> eyre::Result<LintScreen> {
    screen_recording(request, None)
}

/// Receives each witness a recording screen judges.
pub type Recorder<'a> = &'a mut dyn FnMut(ScreenedWitness) -> eyre::Result<()>;

/// [`screen`], also handing every composed document as a witness of
/// `chart` to the recorder when given.
///
/// # Errors
///
/// Returns an error when the chart's inputs cannot be loaded or composed.
pub fn screen_recording(
    request: &ScreenRequest<'_>,
    mut witnesses: Option<(&str, Recorder<'_>)>,
) -> eyre::Result<LintScreen> {
    let mut coverage = LintCoverage {
        imported_rows: request.imported.len(),
        roster_sha256: request.roster_sha256.clone(),
        chart_sha256: tree_sha256(request.chart)?,
        baseline_sha256: sha256_hex(&request.schemas.baseline),
        candidate_sha256: sha256_hex(&request.schemas.candidate),
        candidate_manifest_sha256: request.schemas.manifest_sha256.clone(),
        ..LintCoverage::default()
    };
    let inputs = AcceptanceInputs::load(request.chart)?;
    let rows = screening_rows(request, &inputs, &mut coverage)?;
    coverage.distinct_overlays = rows.len();
    let mut flips = Vec::new();
    if rows.len() > MAX_OVERLAYS || rows.len().saturating_mul(3) > request.available_probes {
        coverage.failures.push(format!(
            "lint lane cap would omit {} overlays (remaining document budget {})",
            rows.len(),
            request.available_probes
        ));
        return Ok(LintScreen { coverage, flips });
    }
    let defaults = default_documents(&inputs)?;
    for (_, row) in rows {
        coverage.rows_screened += row.origins.len();
        coverage.rows.push(RowEvidence {
            origins: row.origins.clone(),
            canonical_overlay_sha256: sha256_hex(&serde_json::to_vec(&row.overlay)?),
            values_file_sha256: sha256_hex(&row.values_file),
            kubernetes_version: row.kubernetes_version.clone(),
            contexts: row.contexts.clone(),
        });
        for kind in AcceptanceDocument::ALL {
            let counts = coverage.documents.entry(format!("{kind:?}")).or_default();
            match inputs.compose(row.overlay.clone(), kind) {
                Ok(values) => {
                    counts.composed += 1;
                    counts.screened += 1;
                    let lint_control = row.lint_control && kind == AcceptanceDocument::LintRaw;
                    let (baseline_accepts, candidate_accepts) =
                        request.schemas.profiles.document_verdicts(&values.root);
                    let transition = request.schemas.profiles.screen_document(&values.root);
                    let flip = transition || (lint_control && !candidate_accepts);
                    if !flip && witnesses.is_none() {
                        continue;
                    }
                    let candidate_violations = witnesses.as_ref().map_or_else(Vec::new, |_| {
                        request
                            .schemas
                            .profiles
                            .candidate_violations(&values.root, transition)
                    });
                    let probe = document_probe(&row, kind, lint_control, values.root, &defaults)?;
                    if let Some((chart, record)) = witnesses.as_mut() {
                        record(ScreenedWitness {
                            chart: (*chart).to_string(),
                            instance: None,
                            composed: probe
                                .document
                                .as_ref()
                                .map_or(Value::Null, |document| document.values.clone()),
                            kubernetes_version: row.kubernetes_version.clone(),
                            probe: probe.clone(),
                            baseline_accepts,
                            candidate_accepts,
                            screened: flip,
                            transition,
                            candidate_violations,
                        })?;
                    }
                    if flip {
                        counts.flips += 1;
                        flips.push(probe);
                    }
                }
                Err(ValuesError::NotValidated(reason)) => {
                    counts.not_validated += 1;
                    coverage
                        .inapplicable
                        .push(format!("{:?} {kind:?}: {reason}", row.origins));
                }
                Err(err) => {
                    counts.errors += 1;
                    coverage
                        .failures
                        .push(format!("{:?} {kind:?}: {err}", row.origins));
                }
            }
        }
    }
    Ok(LintScreen { coverage, flips })
}

/// The lint lane's probe for `row`'s `kind` document `values`.
fn document_probe(
    row: &OverlayRow,
    kind: AcceptanceDocument,
    lint_control: bool,
    values: Value,
    defaults: &BTreeMap<AcceptanceDocument, Option<Value>>,
) -> eyre::Result<ScreenedProbe> {
    // Full context identity cannot collide with legacy path/class roster names.
    let name = format!(
        "document={kind:?} transport=values-file kube={} overlay={}",
        row.kubernetes_version,
        serde_json::to_string(&row.overlay)?
    );
    Ok(ScreenedProbe {
        name,
        overlay: row.overlay.clone(),
        document: Some(ScreenedDocument {
            kind,
            lint_control,
            values,
            defaults: defaults.get(&kind).cloned().flatten(),
            kubernetes_version: row.kubernetes_version.clone(),
            origins: row.origins.clone(),
            values_file: row.values_file.clone(),
            control_overlays: row
                .contexts
                .iter()
                .map(|context| {
                    (
                        context.canonical_overlay_sha256.clone(),
                        context.overlay.clone(),
                    )
                })
                .collect::<BTreeMap<_, _>>()
                .into_values()
                .collect(),
        }),
    })
}

fn screening_rows(
    request: &ScreenRequest<'_>,
    inputs: &AcceptanceInputs,
    coverage: &mut LintCoverage,
) -> eyre::Result<BTreeMap<String, OverlayRow>> {
    let probes = inputs.dependency_probes()?;
    coverage.builtin_rows = probes.len();
    let mut rows = BTreeMap::<String, OverlayRow>::new();
    for probe in probes {
        insert_row(
            &mut rows,
            &probe.overlay,
            serde_json::to_vec(&probe.overlay)?,
            request.kubernetes_version.to_string(),
            format!("dependency:{:?}:{:?}", probe.scope, probe.kind),
            &json!({}),
            probe.kind == DependencyProbeKind::PartialOverride,
        )?;
    }
    for row in request.imported {
        eyre::ensure!(
            row.chart_sha256 == coverage.chart_sha256
                && row.baseline_sha256 == coverage.baseline_sha256
                && row.candidate_sha256 == coverage.candidate_sha256,
            "context row {} input hashes mismatch",
            row.id
        );
        insert_row(
            &mut rows,
            &row.overlay,
            row.values_file.as_bytes().to_vec(),
            row.kubernetes_version.clone(),
            format!("roster:{}", row.id),
            &row.context_overlay,
            false,
        )?;
    }
    Ok(rows)
}

fn insert_row(
    rows: &mut BTreeMap<String, OverlayRow>,
    overlay: &Value,
    values_file: Vec<u8>,
    kubernetes_version: String,
    origin: String,
    context: &Value,
    lint_control: bool,
) -> eyre::Result<()> {
    let key = serde_json::to_string(&json!([kubernetes_version, sha256_hex(&values_file)]))?;
    let row = rows.entry(key).or_insert(OverlayRow {
        overlay: overlay.clone(),
        values_file,
        kubernetes_version,
        origins: Vec::new(),
        contexts: Vec::new(),
        lint_control: false,
    });
    row.lint_control |= lint_control;
    row.contexts.push(ContextEvidence {
        origin: origin.clone(),
        overlay: context.clone(),
        canonical_overlay_sha256: sha256_hex(&serde_json::to_vec(context)?),
    });
    row.origins.push(origin);
    Ok(())
}

fn default_documents(
    inputs: &AcceptanceInputs,
) -> eyre::Result<BTreeMap<AcceptanceDocument, Option<Value>>> {
    let mut defaults = BTreeMap::new();
    for kind in AcceptanceDocument::ALL {
        let document = match inputs.compose(json!({}), kind) {
            Ok(values) => Some(values.root),
            Err(ValuesError::NotValidated(_)) => None,
            Err(err) => return Err(err.into()),
        };
        defaults.insert(kind, document);
    }
    Ok(defaults)
}

/// How a chart's screened lint documents are accounted.
#[derive(Clone, Copy)]
pub enum LintAccounting {
    /// Helm adjudicates every screened document, forced audits included.
    Live,
    /// The schemas alone judge.
    SchemaOnly {
        /// Whether the baseline and candidate artifacts are byte-identical.
        identical_schemas: bool,
    },
}

impl LintAccounting {
    /// The accounting for a run that adjudicates live or not.
    #[must_use]
    pub fn new(adjudicate_live: bool, identical_schemas: bool) -> Self {
        if adjudicate_live {
            Self::Live
        } else {
            Self::SchemaOnly { identical_schemas }
        }
    }
}

/// A chart's screened probes so far.
pub struct ScreenedProbes {
    /// Probes and documents screened.
    pub probes_checked: usize,
    /// Flips judged by the schemas alone.
    pub flips: Vec<String>,
    /// Probes whose values file could not be composed.
    pub failures: Vec<String>,
    /// Flips handed to Helm.
    pub screened: Vec<ScreenedProbe>,
}

impl ScreenedProbes {
    /// Adds a replayed witness that screening proposes and no case in
    /// `known` (discovered or already screened) covers, under `accounting`.
    pub fn append_replayed(
        &mut self,
        witness: &ScreenedWitness,
        known: &BTreeSet<String>,
        accounting: LintAccounting,
        coverage: &mut ProbeCoverage,
    ) {
        self.probes_checked += 1;
        if !witness.screened || known.contains(&super::probe_key(&witness.chart, &witness.probe)) {
            return;
        }
        match accounting {
            LintAccounting::Live => self.screened.push(witness.probe.clone()),
            LintAccounting::SchemaOnly { identical_schemas } => {
                let forced = witness
                    .probe
                    .document
                    .as_ref()
                    .is_some_and(|document| document.lint_control);
                if forced && identical_schemas {
                    if let Some(lint) = coverage.lint.as_mut() {
                        lint.deferred_live_audits.push(witness.probe.name.clone());
                    }
                    return;
                }
                self.flips.push(format!(
                    "{}: before={}, after={}",
                    witness.case(),
                    witness.baseline_accepts,
                    witness.candidate_accepts
                ));
            }
        }
    }

    /// Adds the lint lane's screening under `accounting`.
    pub fn append_lint(
        &mut self,
        lint: eyre::Result<LintScreen>,
        schemas: &LintSchemas,
        coverage: &mut ProbeCoverage,
        mode: LintAccounting,
    ) {
        let mut lint = match lint {
            Ok(lint) => lint,
            Err(err) => {
                coverage.lint = Some(LintCoverage {
                    failures: vec![err.to_string()],
                    ..Default::default()
                });
                return;
            }
        };
        self.probes_checked += lint.coverage.probes();
        for probe in lint.flips {
            let LintAccounting::SchemaOnly { identical_schemas } = mode else {
                self.screened.push(probe);
                continue;
            };
            if let Some(document) = probe.document {
                // Byte-identical schemas give every document identical verdicts, so a forced
                // audit is no transition; only live mode can run it.
                if document.lint_control && identical_schemas {
                    lint.coverage.deferred_live_audits.push(probe.name);
                    continue;
                }
                let (before, after) = schemas.profiles.document_verdicts(&document.values);
                self.flips.push(format!(
                    "{}: {}: before={before}, after={after}",
                    coverage.label, probe.name
                ));
            }
        }
        coverage.lint = Some(lint.coverage);
    }
}

/// Screens both schemas, recording every reachable probe when requested.
///
/// # Errors
///
/// Returns a recording failure immediately, before judging another probe.
pub fn screen_template_probes(
    chart_dir: &std::path::Path,
    profiles: &ProfileSchemas,
    defaults: &serde_json::Value,
    probes: Vec<(String, ProbeInstance)>,
    coverage: &mut ProbeCoverage,
    adjudicate_live: bool,
    mut witnesses: Option<(&str, Recorder<'_>)>,
) -> eyre::Result<ScreenedProbes> {
    let label = &coverage.label;
    let probes_checked = probes.len();
    let mut flips = Vec::new();
    let mut failures = Vec::new();
    let mut screened = Vec::new();
    for (probe_name, probe) in probes {
        if matches!(probe, ProbeInstance::Coalesced(_)) {
            coverage.composed_probes += 1;
        }
        let (overlay, probe) = match probe.helm_values_file(chart_dir, defaults) {
            Ok(ProbeValuesFile::Reachable(overlay)) => (overlay, probe),
            Ok(ProbeValuesFile::ReachableAs { values, document }) => {
                (values, ProbeInstance::Coalesced(document))
            }
            Ok(ProbeValuesFile::Unreachable(reason)) => {
                coverage.unreachable_probes.push(UnreachableProbe {
                    probe: probe_name,
                    reason,
                });
                continue;
            }
            Err(error) => {
                failures.push(format!("{label}: {probe_name}: {error}"));
                continue;
            }
        };
        let flip = profiles.screens_a_flip(&probe);
        if let Some((kubernetes_version, record)) = witnesses.as_mut() {
            let (baseline_accepts, candidate_accepts) = profiles.verdicts(&probe);
            record(ScreenedWitness {
                chart: label.clone(),
                probe: ScreenedProbe {
                    name: probe_name.clone(),
                    overlay: overlay.clone(),
                    document: None,
                },
                candidate_violations: profiles
                    .candidate_violations(&profiles.composed(&probe), flip),
                composed: profiles.composed(&probe),
                instance: Some(probe.clone()),
                kubernetes_version: (*kubernetes_version).to_string(),
                baseline_accepts,
                candidate_accepts,
                screened: flip,
                transition: flip,
            })?;
        }
        if !flip {
            continue;
        }
        if adjudicate_live {
            // Screening proposes an overlay; only Helm can establish its coalesced document.
            screened.push(ScreenedProbe {
                name: probe_name,
                overlay,
                document: None,
            });
        } else {
            let (before, after) = profiles.verdicts(&probe);
            flips.push(format!(
                "{label}: {probe_name}: before={before}, after={after}"
            ));
        }
    }
    Ok(ScreenedProbes {
        probes_checked,
        flips,
        failures,
        screened,
    })
}

/// Compiles both profiles, generates the chart's probe battery and screens
/// it, returning the screened flips for Helm when `adjudicate_live`.
///
/// # Errors
///
/// Returns an error when the chart's inputs cannot be loaded, compiled or staged.
pub fn screen_chart(
    chart: ComparedChart,
    adjudicate_live: bool,
    roster: Option<&ImportedRoster>,
) -> eyre::Result<(ScreenedChart, Option<LiveChart>)> {
    screen_chart_recording(chart, adjudicate_live, roster, None, None)
}

/// [`screen_chart`], also judging the chart's replayed witnesses outside
/// every discovery cap and, when `sink` is given, handing it every
/// discovered and replayed witness as soon as it is judged.
///
/// # Errors
///
/// Returns an error when the chart's inputs cannot be loaded, compiled or
/// staged, or a replayed witness no longer composes to its recorded document.
#[tracing::instrument(
    target = "helm_schema::acceptance",
    skip_all,
    fields(chart = %chart.label, lane = "screen", adjudicate_live)
)]
#[expect(
    clippy::too_many_lines,
    reason = "one chart's screening reads as one sequence: inputs, both lanes, replay and the live hand-off"
)]
pub fn screen_chart_recording(
    chart: ComparedChart,
    adjudicate_live: bool,
    roster: Option<&ImportedRoster>,
    sink: Option<&WitnessSink>,
    runner: Option<&'static crate::helm::invocation::HelmRunner>,
) -> eyre::Result<(ScreenedChart, Option<LiveChart>)> {
    let started = std::time::Instant::now();
    let ComparedChart {
        label,
        chart_relative_path,
        inputs,
        annotation,
        replay,
    } = chart;
    let candidate_path = inputs.candidate_path().map(Path::to_path_buf);
    let LoadedInputs {
        baseline,
        baseline_bytes,
        candidate: current,
        candidate_bytes,
        defaults,
    } = inputs.load()?;
    let profiles = ProfileSchemas::compile(&baseline, &current, defaults.clone())?;
    let lint_compilation = std::time::Instant::now();
    let lint_schemas =
        LintSchemas::compile_bytes(&baseline_bytes, &current, candidate_path.as_deref())?;
    let lint_compilation_ms = elapsed_ms(lint_compilation);
    let (probes, mut coverage) = structural_probe_battery_with_coverage(
        &chart_relative_path,
        &defaults,
        &[&baseline, &current],
    )?;
    coverage.label = label.clone();
    let chart_dir = HelmChartDir::stage(&chart_relative_path)?;
    let mut cost = ChartCost {
        label: label.clone(),
        context_ms: elapsed_ms(started),
        screening_ms: 0,
        lint_screening_ms: 0,
        lint_compilation_ms,
        preparation_ms: 0,
        adjudication_ms: 0,
        prepared_trees: None,
        helm: std::collections::BTreeMap::new(),
    };
    let version = crate::helm::kubernetes_version::chart_kubernetes_version(chart_dir.path())?;
    let mut record = |witness: ScreenedWitness| {
        if let Some(sink) = sink {
            sink(witness)?;
        }
        Ok(())
    };
    let screening = std::time::Instant::now();
    let mut result = screen_template_probes(
        chart_dir.path(),
        &profiles,
        &defaults,
        probes,
        &mut coverage,
        adjudicate_live,
        sink.is_some()
            .then_some((version, &mut record as Recorder<'_>)),
    )?;
    let lint_started = std::time::Instant::now();
    let mut imported = roster
        .and_then(|roster| roster.rows.get(&label))
        .map_or_else(Vec::new, Clone::clone);
    if roster.is_some_and(|roster| roster.rebind) {
        let chart_sha256 = tree_sha256(chart_dir.path())?;
        for row in &mut imported {
            row.chart_sha256.clone_from(&chart_sha256);
            row.baseline_sha256 = sha256_hex(&lint_schemas.baseline);
            row.candidate_sha256 = sha256_hex(&lint_schemas.candidate);
        }
    }
    let lint = screen_recording(
        &ScreenRequest {
            chart: chart_dir.path(),
            schemas: &lint_schemas,
            kubernetes_version: version,
            imported: &imported,
            roster_sha256: roster.map(|roster| roster.sha256.clone()),
            available_probes: MAX_PROBES_PER_CHART.saturating_sub(result.probes_checked),
        },
        sink.is_some()
            .then_some((label.as_str(), &mut record as Recorder<'_>)),
    );
    if sink.is_some()
        && let Err(err) = &lint
    {
        return Err(eyre::eyre!("record lint: {err:#}"));
    }
    let accounting = LintAccounting::new(adjudicate_live, baseline_bytes == candidate_bytes);
    result.append_lint(lint, &lint_schemas, &mut coverage, accounting);
    cost.lint_screening_ms = elapsed_ms(lint_started);
    let known = result
        .screened
        .iter()
        .map(|probe| super::probe_key(&coverage.label, probe))
        .collect::<BTreeSet<_>>();
    let mut inputs = None;
    for witness in replay {
        let witness = witness?;
        eyre::ensure!(
            witness.probe.document.is_some() || witness.kubernetes_version == version,
            "replayed template witness {} has another Kubernetes version; use an explicit document witness",
            witness.case()
        );
        let witness = rejudge(
            chart_dir.path(),
            &profiles,
            &lint_schemas,
            &mut inputs,
            witness,
        )?;
        result.append_replayed(&witness, &known, accounting, &mut coverage);
        if let Some(sink) = sink {
            sink(witness)?;
        }
    }
    cost.screening_ms = elapsed_ms(screening);
    let ScreenedProbes {
        probes_checked,
        flips,
        failures,
        screened,
    } = result;
    let mut screened = screened;
    if sink.is_some() {
        let mut unique = BTreeSet::new();
        screened.retain(|probe| unique.insert(super::probe_key(&label, probe)));
    }
    let live = (!screened.is_empty()).then(|| LiveChart {
        label: label.clone(),
        path: test_util::workspace_testdata()
            .join("charts")
            .join(&chart_relative_path),
        profiles,
        flips: screened,
        lint_schemas,
        helm: std::sync::Arc::new(ChartHelm {
            runner,
            ..ChartHelm::default()
        }),
        annotation,
    });
    let screened = ScreenedChart {
        label,
        coverage,
        probes_checked,
        flips,
        failures,
        screened_flips: live.as_ref().map_or(0, |live| live.flips.len()),
        helm: live.as_ref().map(|live| live.helm.clone()),
        cost,
    };
    Ok((screened, live))
}

/// A replayed witness judged again by both schemas, first recomposed from
/// its overlay and required to reproduce its recorded document; `inputs`
/// caches the chart's composition inputs across witnesses.
///
/// # Errors
///
/// Returns an error when the witness no longer composes or composes to
/// another document.
pub fn rejudge(
    chart_dir: &Path,
    profiles: &ProfileSchemas,
    lint: &LintSchemas,
    inputs: &mut Option<AcceptanceInputs>,
    mut witness: ScreenedWitness,
) -> eyre::Result<ScreenedWitness> {
    let case = witness.case();
    let (verdicts, transition, screened) = match (&witness.instance, &mut witness.probe.document) {
        (Some(instance), None) => {
            eyre::ensure!(
                profiles.composed(instance) == witness.composed,
                "replayed witness {case} composes to another document"
            );
            let transition = profiles.screens_a_flip(instance);
            witness.candidate_violations =
                profiles.candidate_violations(&witness.composed, transition);
            (profiles.verdicts(instance), transition, transition)
        }
        (None, Some(document)) => {
            let inputs = match inputs {
                Some(inputs) => inputs,
                None => inputs.insert(AcceptanceInputs::load(chart_dir)?),
            };
            let composed = inputs
                .compose(witness.probe.overlay.clone(), document.kind)
                .map_err(|error| eyre::eyre!("replayed witness {case}: {error}"))?;
            eyre::ensure!(
                composed.root == document.values && document.values == witness.composed,
                "replayed witness {case} composes to another document"
            );
            document.defaults = default_documents(inputs)?.remove(&document.kind).flatten();
            let verdicts = lint.profiles.document_verdicts(&document.values);
            let transition = lint.profiles.screen_document(&document.values);
            witness.candidate_violations = lint
                .profiles
                .candidate_violations(&document.values, transition);
            (
                verdicts,
                transition,
                transition || (document.lint_control && !verdicts.1),
            )
        }
        (Some(_), Some(_)) | (None, None) => {
            eyre::bail!("replayed witness {case} names neither or both lanes")
        }
    };
    (witness.baseline_accepts, witness.candidate_accepts) = verdicts;
    witness.screened = screened;
    witness.transition = transition;
    Ok(witness)
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct DocumentInput<'a> {
    values: &'a [u8],
    kubernetes_version: &'a str,
}

/// One lint invocation observes both lint documents, even when templates are nondeterministic.
/// Chart and schema bytes are fixed by the owning `LiveChart`; imported values retain exact bytes.
pub fn probe_groups(live: &LiveChart) -> Vec<Vec<usize>> {
    let mut documents = std::collections::BTreeMap::<DocumentInput<'_>, Vec<usize>>::new();
    let mut groups = Vec::new();
    for (flip, probe) in live.flips.iter().enumerate() {
        if let Some(document) = &probe.document {
            documents
                .entry(DocumentInput {
                    values: &document.values_file,
                    kubernetes_version: &document.kubernetes_version,
                })
                .or_default()
                .push(flip);
        } else {
            groups.push(vec![flip]);
        }
    }
    groups.extend(documents.into_values());
    groups.sort_by_key(|group| group.first().copied());
    groups
}
