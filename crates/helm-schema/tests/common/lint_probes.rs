//! Dependency and imported-context screening against Helm's three schema documents.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_test_support::{
    helm::invocation::tree_sha256, manifest, source_digest::sha256_hex,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use test_util::helm_values::{AcceptanceDocument, AcceptanceInputs, ValuesError};

use crate::harness::ProfileSchemas;

pub(crate) const CONTEXT_ROSTER_VAR: &str = "SCHEMA_ACCEPTANCE_CONTEXT_ROSTER";
const MAX_OVERLAYS: usize = 8_192;
const MAX_ROSTER_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContextRow {
    pub(crate) id: String,
    pub(crate) chart: String,
    pub(crate) overlay: Value,
    pub(crate) transport: String,
    pub(crate) kubernetes_version: String,
    pub(crate) chart_sha256: String,
    pub(crate) baseline_sha256: String,
    pub(crate) candidate_sha256: String,
    pub(crate) overlay_sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContextRoster {
    pub(crate) version: u32,
    pub(crate) rows: Vec<ContextRow>,
}

pub(crate) struct ImportedRoster {
    pub(crate) sha256: String,
    pub(crate) rows: BTreeMap<String, Vec<ContextRow>>,
}

impl ImportedRoster {
    pub(crate) fn read(path: &Path, charts: &BTreeSet<String>) -> eyre::Result<Self> {
        eyre::ensure!(
            std::fs::metadata(path)?.len() <= MAX_ROSTER_BYTES,
            "context roster exceeds byte cap"
        );
        let bytes = std::fs::read(path)?;
        let roster: ContextRoster = serde_json::from_slice(&bytes)?;
        eyre::ensure!(
            roster.version == 1,
            "unsupported context roster version {}",
            roster.version
        );
        eyre::ensure!(!roster.rows.is_empty(), "empty context roster");
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
                sha256_hex(&serde_json::to_vec(&row.overlay)?) == row.overlay_sha256,
                "context row {} overlay hash mismatch",
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
        })
    }
}

/// The exact root schemas installed for lint, derived without rerunning inference.
pub(crate) struct LintSchemas {
    pub(crate) baseline: Vec<u8>,
    pub(crate) candidate: Vec<u8>,
    pub(crate) profiles: ProfileSchemas,
    pub(crate) manifest_sha256: Option<String>,
}

impl LintSchemas {
    pub(crate) fn compile(
        baseline: &Value,
        candidate: &Value,
        candidate_path: Option<&Path>,
    ) -> eyre::Result<Self> {
        let baseline = helm_ready(baseline)?;
        let candidate = if let Some(path) = candidate_path {
            verified_companion(path, candidate)?
        } else {
            helm_ready(candidate)?
        };
        let profiles = ProfileSchemas::compile(
            &serde_json::from_slice(&baseline)?,
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

pub(crate) fn helm_ready(schema: &Value) -> eyre::Result<Vec<u8>> {
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
pub(crate) struct DocumentCoverage {
    pub(crate) composed: usize,
    pub(crate) not_validated: usize,
    pub(crate) errors: usize,
    pub(crate) screened: usize,
    pub(crate) flips: usize,
    pub(crate) adjudicated: usize,
    pub(crate) adjudication_failures: usize,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct LintCoverage {
    pub(crate) builtin_rows: usize,
    pub(crate) imported_rows: usize,
    pub(crate) rows_screened: usize,
    pub(crate) distinct_overlays: usize,
    pub(crate) documents: BTreeMap<String, DocumentCoverage>,
    pub(crate) roster_sha256: Option<String>,
    pub(crate) chart_sha256: String,
    pub(crate) baseline_sha256: String,
    pub(crate) candidate_sha256: String,
    pub(crate) candidate_manifest_sha256: Option<String>,
    pub(crate) failures: Vec<String>,
    pub(crate) inapplicable: Vec<String>,
    pub(crate) rows: Vec<RowEvidence>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct RowEvidence {
    pub(crate) origins: Vec<String>,
    pub(crate) overlay_sha256: String,
    pub(crate) kubernetes_version: String,
}

impl LintCoverage {
    pub(crate) fn validate(&self) -> eyre::Result<()> {
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
                    || (kind == AcceptanceDocument::LintRaw
                        && counts.not_validated == self.distinct_overlays),
                "no {kind:?} documents screened"
            );
        }
        Ok(())
    }

    pub(crate) fn validate_live(&self) -> eyre::Result<()> {
        self.validate()?;
        for (kind, counts) in &self.documents {
            eyre::ensure!(
                counts.adjudication_failures == 0 && counts.flips == counts.adjudicated,
                "incomplete {kind} adjudication: {counts:?}"
            );
        }
        Ok(())
    }

    pub(crate) fn probes(&self) -> usize {
        self.documents.values().map(|counts| counts.screened).sum()
    }
}

#[derive(Clone)]
pub(crate) struct ScreenedProbe {
    pub(crate) name: String,
    pub(crate) overlay: Value,
    /// `None` retains the legacy exact-template adjudication path.
    pub(crate) document: Option<ScreenedDocument>,
}

#[derive(Clone)]
pub(crate) struct ScreenedDocument {
    pub(crate) kind: AcceptanceDocument,
    pub(crate) values: Value,
    pub(crate) defaults: Option<Value>,
    pub(crate) kubernetes_version: String,
    pub(crate) origins: Vec<String>,
}

struct OverlayRow {
    overlay: Value,
    kubernetes_version: String,
    origins: Vec<String>,
}

pub(crate) struct LintScreen {
    pub(crate) coverage: LintCoverage,
    pub(crate) flips: Vec<ScreenedProbe>,
}

pub(crate) struct ScreenRequest<'a> {
    pub(crate) chart: &'a Path,
    pub(crate) schemas: &'a LintSchemas,
    pub(crate) kubernetes_version: &'a str,
    pub(crate) imported: &'a [ContextRow],
    pub(crate) roster_sha256: Option<String>,
    pub(crate) available_probes: usize,
}

pub(crate) fn screen(request: &ScreenRequest<'_>) -> eyre::Result<LintScreen> {
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
    let probes = inputs.dependency_probes()?;
    coverage.builtin_rows = probes.len();
    let mut rows = BTreeMap::<String, OverlayRow>::new();
    for probe in probes {
        insert_row(
            &mut rows,
            probe.overlay,
            request.kubernetes_version.to_string(),
            format!("dependency:{:?}:{:?}", probe.scope, probe.kind),
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
            row.overlay.clone(),
            row.kubernetes_version.clone(),
            format!("roster:{}", row.id),
        )?;
    }
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
            overlay_sha256: sha256_hex(&serde_json::to_vec(&row.overlay)?),
            kubernetes_version: row.kubernetes_version.clone(),
        });
        for kind in AcceptanceDocument::ALL {
            let counts = coverage.documents.entry(format!("{kind:?}")).or_default();
            match inputs.compose(row.overlay.clone(), kind) {
                Ok(values) => {
                    counts.composed += 1;
                    counts.screened += 1;
                    if request.schemas.profiles.screen_document(&values.root) {
                        counts.flips += 1;
                        // Full context identity cannot collide with legacy path/class roster names.
                        let name = format!(
                            "document={kind:?} transport=values-file kube={} overlay={}",
                            row.kubernetes_version,
                            serde_json::to_string(&row.overlay)?
                        );
                        flips.push(ScreenedProbe {
                            name,
                            overlay: row.overlay.clone(),
                            document: Some(ScreenedDocument {
                                kind,
                                values: values.root,
                                defaults: defaults.get(&kind).cloned().flatten(),
                                kubernetes_version: row.kubernetes_version.clone(),
                                origins: row.origins.clone(),
                            }),
                        });
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

fn insert_row(
    rows: &mut BTreeMap<String, OverlayRow>,
    overlay: Value,
    kubernetes_version: String,
    origin: String,
) -> eyre::Result<()> {
    let key = serde_json::to_string(&json!([kubernetes_version, overlay]))?;
    let row = rows.entry(key).or_insert(OverlayRow {
        overlay,
        kubernetes_version,
        origins: Vec::new(),
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
