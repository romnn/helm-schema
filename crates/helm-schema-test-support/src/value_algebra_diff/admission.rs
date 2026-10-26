//! W1 admission cells as differential witnesses.
//!
//! A capture generates every cell's schema with that revision's own CLI and
//! records the verdict on the cell's values composed over the chart defaults,
//! checked against the document pinned Helm coalesced. Each cell's pinned Helm
//! class is a complete oracle, so main's and the candidate's verdicts classify
//! like any other witness. The suite is frozen and bound into every capture by
//! hash; a changed suite is another experiment.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::DiffError;
use super::compare::Class;
use super::history::{canonical_json, canonical_sha256};
use crate::admission::{self, Cell, DeclaredTypes, HelmClass, Suite, Verdict};
use crate::registry::PROVIDER_BUNDLE;
use crate::source_digest::sha256_hex;

/// Concurrent CLI generations.
const WORKERS: usize = 4;

/// One cell's measured verdict in one capture.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionRecord {
    /// The cell id.
    pub cell: String,
    /// `assert` or `annotate`.
    pub declared_types: String,
    /// `accept` or `reject`.
    pub verdict: String,
    /// The canonical identities of the assertions a rejection fails.
    pub violations: Vec<String>,
    /// [`admission::inputs_sha256`] of the cell.
    pub inputs_sha256: String,
    /// [`admission::document_sha256`] of the validated document.
    pub document_sha256: String,
}

/// One cell's outcome in one comparison.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdmissionOutcome {
    /// The witness id: the cell, its mode and its input and document hashes.
    pub witness: String,
    /// The cell id.
    pub cell: String,
    /// Pinned Helm's class, `pass` or `reject`.
    pub helm_class: String,
    /// Main's verdict.
    pub main_verdict: String,
    /// The candidate's verdict.
    pub candidate_verdict: String,
    /// The class.
    pub class: Class,
    /// `false-accept` or `false-reject` for a regression or a closure.
    pub direction: Option<String>,
    /// SHA-256 of the candidate's verdict.
    pub answer_sha256: String,
}

/// The suite under `root`, refused unless its roster, obligations,
/// coalesced documents and files are consistent.
///
/// # Errors
///
/// [`DiffError::Identity`] for an inconsistent suite;
/// [`DiffError::Request`] when it cannot be read.
pub fn read_checked_suite(root: &Path) -> Result<Suite, DiffError> {
    let suite = admission::read_suite(root)
        .map_err(|error| DiffError::Request(format!("admission suite: {error:#}")))?;
    let mut problems = suite.roster_mismatches();
    problems.extend(suite.obligation_mismatches());
    let infrastructure = |error: color_eyre::Report| {
        DiffError::Infrastructure(format!("admission suite: {error:#}"))
    };
    problems.extend(suite.coalesced_mismatches().map_err(infrastructure)?);
    problems.extend(suite.empty_files().map_err(infrastructure)?);
    if problems.is_empty() {
        Ok(suite)
    } else {
        Err(DiffError::Identity(format!(
            "the admission suite is inconsistent: {}",
            problems.join("; ")
        )))
    }
}

/// Measures every cell of the suite at `root` with the CLI `cli`, its
/// provider bundle at `testdata`, in the new scratch directory `scratch`.
///
/// # Errors
///
/// [`DiffError::Identity`] for an inconsistent suite; [`DiffError::Infrastructure`]
/// when a generation, composition or validation fails.
pub fn measure(
    root: &Path,
    testdata: &Path,
    cli: &Path,
    scratch: &Path,
) -> Result<Vec<AdmissionRecord>, DiffError> {
    let suite = read_checked_suite(root)?;
    std::fs::create_dir_all(scratch)
        .map_err(|error| DiffError::Infrastructure(format!("{}: {error}", scratch.display())))?;
    let jobs = suite
        .cells
        .iter()
        .map(|cell| (cell.chart.clone(), cell.declared_types))
        .collect::<BTreeSet<_>>();
    let bundle = testdata.join(PROVIDER_BUNDLE);
    let queue = Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>());
    let schemas = Mutex::new(BTreeMap::new());
    std::thread::scope(|scope| {
        for _ in 0..WORKERS {
            scope.spawn(|| {
                while let Some((index, (chart, mode))) =
                    queue.lock().ok().and_then(|mut queue| queue.pop())
                {
                    let out = scratch.join(format!("schema-{index}.json"));
                    let result = generate(cli, &root.join(&chart), mode, &bundle, &out, scratch);
                    if let Ok(mut schemas) = schemas.lock() {
                        schemas.insert((chart, mode), result);
                    }
                }
            });
        }
    });
    let schemas = schemas
        .into_inner()
        .map_err(|_| DiffError::Infrastructure("an admission worker panicked".into()))?;
    let mut records = Vec::with_capacity(suite.cells.len());
    for cell in &suite.cells {
        let schema = schemas
            .get(&(cell.chart.clone(), cell.declared_types))
            .ok_or_else(|| DiffError::Infrastructure(format!("{}: never generated", cell.id)))?
            .as_ref()
            .map_err(|reason| DiffError::Infrastructure(format!("{}: {reason}", cell.id)))?;
        let document = admission::compose(&suite, cell, scratch)
            .map_err(|error| DiffError::Infrastructure(format!("{}: {error:#}", cell.id)))?;
        let verdict = if schema.is_valid(&document) {
            Verdict::Accept
        } else {
            Verdict::Reject
        };
        records.push(AdmissionRecord {
            cell: cell.id.clone(),
            declared_types: cell.declared_types.as_arg().to_string(),
            verdict: verdict_name(verdict).to_string(),
            violations: crate::acceptance::discovery::violation_identities(schema, &document),
            inputs_sha256: admission::inputs_sha256(root, cell)
                .map_err(|error| DiffError::Infrastructure(format!("{}: {error:#}", cell.id)))?,
            document_sha256: admission::document_sha256(&document)
                .map_err(|error| DiffError::Infrastructure(format!("{}: {error:#}", cell.id)))?,
        });
    }
    records.sort();
    Ok(records)
}

/// Generates one (chart, mode) schema with `cli` and compiles it.
fn generate(
    cli: &Path,
    chart: &Path,
    mode: DeclaredTypes,
    bundle: &Path,
    out: &Path,
    scratch: &Path,
) -> Result<jsonschema::Validator, String> {
    let arguments = admission::generation_arguments(mode)
        .into_iter()
        .map(|argument| {
            argument
                .replace("<chart>", &chart.to_string_lossy())
                .replace("<bundle>", &bundle.to_string_lossy())
                .replace("<out>", &out.to_string_lossy())
        })
        .collect::<Vec<_>>();
    let output = Command::new(cli)
        .env_clear()
        .env("HOME", scratch)
        .env("XDG_CONFIG_HOME", scratch)
        .env("XDG_CACHE_HOME", scratch)
        .args(&arguments)
        .output()
        .map_err(|error| format!("spawn {}: {error}", cli.display()))?;
    if !output.status.success() {
        return Err(format!(
            "generation failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let schema: Value = std::fs::read(out)
        .map_err(|error| error.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|error| error.to_string()))?;
    jsonschema::validator_for(&schema).map_err(|error| format!("schema does not compile: {error}"))
}

const fn verdict_name(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Accept => "accept",
        Verdict::Reject => "reject",
    }
}

const fn helm_name(helm: HelmClass) -> &'static str {
    match helm {
        HelmClass::Pass => "pass",
        HelmClass::Reject => "reject",
    }
}

/// The witness id of a measured cell.
///
/// # Errors
///
/// [`DiffError::Infrastructure`] when it cannot be serialized.
pub fn witness_id(record: &AdmissionRecord) -> Result<String, DiffError> {
    canonical_sha256(&json!({"admission_v1": {
        "cell": record.cell,
        "declared_types": record.declared_types,
        "inputs_sha256": record.inputs_sha256,
        "document_sha256": record.document_sha256,
    }}))
}

/// Cells whose measured verdict differs from the verdict the suite records
/// for main: main's capture must measure the suite exactly.
#[must_use]
pub fn main_disagreements(suite: &Suite, main: &[AdmissionRecord]) -> Vec<String> {
    let recorded = suite
        .cells
        .iter()
        .map(|cell| (cell.id.as_str(), verdict_name(cell.main)))
        .collect::<BTreeMap<_, _>>();
    let mut problems = Vec::new();
    for record in main {
        if recorded.get(record.cell.as_str()) != Some(&record.verdict.as_str()) {
            problems.push(format!(
                "admission cell {} measures {} where the suite records main={:?}",
                record.cell,
                record.verdict,
                recorded.get(record.cell.as_str())
            ));
        }
    }
    if main.len() != recorded.len() {
        problems.push(format!(
            "main measured {} admission cells, the suite holds {}",
            main.len(),
            recorded.len()
        ));
    }
    problems
}

/// Classifies every cell from main's and the candidate's verdicts against
/// pinned Helm's class.
///
/// # Errors
///
/// [`DiffError::Identity`] when the captures measured different cells,
/// inputs or documents, or a cell is not in the suite.
pub fn compare(
    suite: &Suite,
    main: &[AdmissionRecord],
    candidate: &[AdmissionRecord],
) -> Result<Vec<AdmissionOutcome>, DiffError> {
    let cells = suite
        .cells
        .iter()
        .map(|cell| (cell.id.as_str(), cell))
        .collect::<BTreeMap<&str, &Cell>>();
    let candidate = candidate
        .iter()
        .map(|record| (record.cell.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    if main.len() != candidate.len() {
        return Err(DiffError::Identity(
            "the captures measured different admission cells".into(),
        ));
    }
    let mut outcomes = Vec::with_capacity(main.len());
    for record in main {
        let other = candidate.get(record.cell.as_str()).ok_or_else(|| {
            DiffError::Identity(format!("{}: measured by main only", record.cell))
        })?;
        if (
            &other.declared_types,
            &other.inputs_sha256,
            &other.document_sha256,
        ) != (
            &record.declared_types,
            &record.inputs_sha256,
            &record.document_sha256,
        ) {
            return Err(DiffError::Identity(format!(
                "{}: the captures composed different inputs",
                record.cell
            )));
        }
        let cell = cells
            .get(record.cell.as_str())
            .ok_or_else(|| DiffError::Identity(format!("{} is not in the suite", record.cell)))?;
        let truth = match cell.helm {
            HelmClass::Pass => "accept",
            HelmClass::Reject => "reject",
        };
        let error = |verdict: &str| {
            if verdict == "accept" {
                "false-accept"
            } else {
                "false-reject"
            }
        };
        let (class, direction) = match (record.verdict == truth, other.verdict == truth) {
            (true, true) => (Class::Identical, None),
            (false, false) if record.violations == other.violations => (Class::Inherited, None),
            // Wrong both times, but failing other assertions: a new wrong answer.
            (false, false) => (Class::Regression, Some("changed-failure")),
            (true, false) => (Class::Regression, Some(error(&other.verdict))),
            (false, true) => (Class::Closure, Some(error(&record.verdict))),
        };
        outcomes.push(AdmissionOutcome {
            witness: witness_id(record)?,
            cell: record.cell.clone(),
            helm_class: helm_name(cell.helm).to_string(),
            main_verdict: record.verdict.clone(),
            candidate_verdict: other.verdict.clone(),
            class,
            direction: direction.map(str::to_string),
            answer_sha256: sha256_hex(&canonical_json(
                &json!({"verdict": other.verdict, "violations": other.violations}),
            )),
        });
    }
    outcomes.sort_by(|left, right| left.witness.cmp(&right.witness));
    Ok(outcomes)
}
