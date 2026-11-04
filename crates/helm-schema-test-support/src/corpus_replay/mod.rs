//! Deterministic Draft 7 replay of saved, already coalesced Helm evidence.

mod dump;
mod evidence;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use jsonschema::Validator;
use serde::Serialize;
use serde_json::Value;

use crate::manifest;
use crate::source_digest::sha256_hex;

/// A replay input or execution failure; invalid schemas never count as rejections.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// A file could not be read or written.
    #[error("access {path}")]
    Io {
        /// File being accessed.
        path: PathBuf,
        /// Filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// An evidence file is malformed.
    #[error("parse {path}")]
    Json {
        /// JSON file being decoded or encoded.
        path: PathBuf,
        /// JSON failure.
        #[source]
        source: serde_json::Error,
    },
    /// A producer dump is stale, incomplete, or corrupt.
    #[error(transparent)]
    Provenance(#[from] manifest::ProvenanceError),
    /// A schema could not be compiled.
    #[error("compile {label}: {message}")]
    Schema {
        /// Schema source.
        label: String,
        /// Validator diagnostic.
        message: String,
    },
    /// Inputs cannot define a complete replay.
    #[error("invalid replay input: {0}")]
    Input(String),
    /// Git could not read a baseline fixture.
    #[error("git baseline {fixture}: {message}")]
    Git {
        /// Fixture or revision being resolved.
        fixture: String,
        /// Git diagnostic.
        message: String,
    },
    /// A chart worker panicked.
    #[error("replay worker panicked")]
    Worker,
}

/// A baseline schema source.
#[derive(Debug, Clone)]
pub enum Baseline {
    /// A complete historical producer dump with verified bytes and recipe inputs.
    Dump(PathBuf),
    /// Committed fixtures at this Git revision.
    Git(String),
}

/// Inputs to a complete saved-evidence replay.
#[derive(Debug, Clone)]
pub struct Options {
    /// Complete producer output with recorded build provenance.
    pub candidate: PathBuf,
    /// Previous schemas.
    pub baseline: Baseline,
    /// Preview evidence root containing `cases/chart-*`.
    pub evidence: PathBuf,
    /// Placement census containing its `documents` class map.
    pub census: PathBuf,
    /// Maximum concurrent charts.
    pub jobs: usize,
}

/// Counts use saved document occurrences; census classes count target/document memberships.
#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct Counts {
    /// Saved document occurrences.
    pub documents: usize,
    /// Previously accepted, now rejected documents known to fail Helm or Kubernetes.
    pub fa_closed: usize,
    /// Previously rejected, now accepted documents known to fail Helm or Kubernetes.
    pub fa_reopened: usize,
    /// Newly rejected documents with saved successful Helm evidence.
    pub new_fr_candidates: usize,
    /// Equal baseline and candidate verdicts.
    pub unchanged: usize,
    /// Changes whose saved Helm outcome is unknown, or successful loosenings.
    pub other_changes: usize,
}

impl Counts {
    fn record(&mut self, row: &Record) {
        self.documents += 1;
        if row.before == row.after {
            self.unchanged += 1;
        } else {
            match row.oracle {
                Oracle::Rejects => {
                    if row.before {
                        self.fa_closed += 1;
                    } else {
                        self.fa_reopened += 1;
                    }
                }
                Oracle::Valid => {
                    if row.before {
                        self.new_fr_candidates += 1;
                    } else {
                        self.other_changes += 1;
                    }
                }
                Oracle::Unknown => self.other_changes += 1,
            }
        }
    }
}

/// A saved oracle result; missing evidence stays unknown.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum Oracle {
    /// Saved Helm or Kubernetes rejection.
    Rejects,
    /// Saved successful Helm result, without a saved Kubernetes rejection.
    Valid,
    /// No decisive saved result.
    Unknown,
}

#[derive(Clone, Copy)]
struct Verdicts {
    before: bool,
    after: bool,
}

/// One document occurrence, with evidence retained for changed verdicts.
#[derive(Debug, Serialize)]
pub struct Record {
    /// File path, optionally followed by a JSON pointer.
    pub source: String,
    /// Canonical document digest used within this chart.
    pub document_sha256: String,
    /// Baseline accepts the saved document.
    pub before: bool,
    /// Candidate accepts the saved document.
    pub after: bool,
    /// Saved oracle outcome; no live inference is performed.
    pub oracle: Oracle,
    /// Census classes; one occurrence can belong to multiple target contexts.
    pub classes: Vec<String>,
}

/// An index entry whose document was not retained in the saved preview.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct MissingReference {
    /// Index file containing the reference.
    pub index: String,
    /// Recorded evidence path that could not be located.
    pub reference: String,
}

/// One chart's replay result.
#[derive(Debug, Serialize)]
pub struct ChartReport {
    /// Corpus chart name.
    pub chart: String,
    /// Exact baseline schema digest before normalization.
    pub baseline_sha256: String,
    /// Exact candidate schema digest.
    pub candidate_sha256: String,
    /// Totals including unchanged occurrences.
    pub counts: Counts,
    /// Counts by census class, preserving target/document memberships.
    pub classes: BTreeMap<String, Counts>,
    /// Distinct saved JSON documents validated once per schema.
    pub distinct_documents: usize,
    /// Dangling index links, excluded from document and verdict counts.
    pub unresolved_index_references: Vec<MissingReference>,
    /// Changed occurrences, including every new false-rejection candidate's evidence path.
    pub changes: Vec<Record>,
}

/// Complete, sorted replay output.
#[derive(Debug, Serialize)]
pub struct Report {
    /// Candidate manifest digest binding this run to the producer.
    pub candidate_manifest_sha256: String,
    /// Exact census input digest.
    pub census_sha256: String,
    /// Resolved Git commit or baseline manifest digest.
    pub baseline: String,
    /// Chart results in lexical order, including charts with no saved evidence.
    pub charts: Vec<ChartReport>,
    /// Corpus totals.
    pub counts: Counts,
    /// Corpus counts by census class.
    pub classes: BTreeMap<String, Counts>,
}

pub(super) fn read(path: &Path) -> Result<Vec<u8>, ReplayError> {
    std::fs::read(path).map_err(|source| ReplayError::Io {
        path: path.into(),
        source,
    })
}
pub(super) fn json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ReplayError> {
    serde_json::from_slice(&read(path)?).map_err(|source| ReplayError::Json {
        path: path.into(),
        source,
    })
}
fn parse(bytes: &[u8], path: &Path) -> Result<Value, ReplayError> {
    serde_json::from_slice(bytes).map_err(|source| ReplayError::Json {
        path: path.into(),
        source,
    })
}

/// Compiles a schema with the repository's pinned validator and explicit Draft 7.
///
/// # Errors
///
/// Returns an error for an invalid schema, including unresolved references.
pub fn compile(schema: &Value, label: &str) -> Result<Validator, ReplayError> {
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .build(schema)
        .map_err(|err| ReplayError::Schema {
            label: label.into(),
            message: err.to_string(),
        })
}

/// Verifies both dumps and replays charts in bounded parallel workers.
///
/// # Errors
///
/// Returns an error for corrupt provenance, missing evidence, compilation or worker failures.
#[tracing::instrument(target = "helm_schema::corpus_replay", skip_all)]
pub fn run(options: &Options) -> Result<Report, ReplayError> {
    if options.jobs == 0 {
        return Err(ReplayError::Input("jobs must be positive".into()));
    }
    let frozen = resolve_options(options)?;
    let options = &frozen;
    let candidate_manifest_sha256 =
        sha256_hex(&read(&options.candidate.join(manifest::MANIFEST_FILE))?);
    let candidate = dump::verify(&options.candidate)?;
    let baseline_identity = match &options.baseline {
        Baseline::Git(revision) => revision.clone(),
        Baseline::Dump(path) => sha256_hex(&read(&path.join(manifest::MANIFEST_FILE))?),
    };
    let baseline = match &options.baseline {
        Baseline::Dump(path) => {
            let baseline = dump::verify(path)?;
            Some(baseline)
        }
        Baseline::Git(_) => None,
    };
    let census_bytes = read(&options.census)?;
    let census: evidence::Census =
        serde_json::from_slice(&census_bytes).map_err(|source| ReplayError::Json {
            path: options.census.clone(),
            source,
        })?;
    let classes = census.index(&options.evidence)?;
    let entries: Vec<_> = candidate
        .artifacts
        .iter()
        .filter(|entry| entry.key.starts_with("chart/") && entry.fixture.is_some())
        .collect();
    validate_census_charts(
        &options.evidence,
        entries
            .iter()
            .filter_map(|entry| entry.key.strip_prefix("chart/")),
        &classes,
    )?;
    let charts = replay_charts(options, &entries, baseline.as_ref(), &classes)?;
    let mut report = Report {
        candidate_manifest_sha256,
        census_sha256: sha256_hex(&census_bytes),
        baseline: baseline_identity,
        charts,
        counts: Counts::default(),
        classes: BTreeMap::new(),
    };
    for chart in &report.charts {
        add_counts(&mut report.counts, &chart.counts);
        for (class, counts) in &chart.classes {
            add_counts(report.classes.entry(class.clone()).or_default(), counts);
        }
    }
    dump::unchanged(&options.candidate, &report.candidate_manifest_sha256)?;
    if let Baseline::Dump(path) = &options.baseline {
        dump::unchanged(path, &report.baseline)?;
    }
    Ok(report)
}

pub(crate) fn validate_census_charts<'a>(
    evidence: &Path,
    charts: impl IntoIterator<Item = &'a str>,
    classes: &BTreeMap<String, Vec<String>>,
) -> Result<(), ReplayError> {
    let roots: Vec<_> = charts
        .into_iter()
        .map(|chart| evidence.join("cases").join(format!("chart-{chart}")))
        .collect();
    if let Some(source) = classes.keys().find(|source| {
        let path = source_path(source);
        !roots.iter().any(|root| path.starts_with(root))
    }) {
        return Err(ReplayError::Input(format!(
            "census document outside replay chart schedule: {source}"
        )));
    }
    Ok(())
}

fn replay_charts(
    options: &Options,
    entries: &[&manifest::ManifestEntry],
    baseline: Option<&manifest::Manifest>,
    classes: &evidence::ClassIndex,
) -> Result<Vec<ChartReport>, ReplayError> {
    let next = AtomicUsize::new(0);
    let mut charts = std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..options.jobs.min(entries.len()) {
            workers.push(scope.spawn(|| {
                let mut results = Vec::new();
                while let Some(entry) = entries.get(next.fetch_add(1, Ordering::Relaxed)) {
                    results.push(replay_chart(options, entry, baseline, classes));
                }
                results
            }));
        }
        let mut results = Vec::new();
        for worker in workers {
            results.extend(worker.join().map_err(|_| ReplayError::Worker)?);
        }
        results.into_iter().collect::<Result<Vec<_>, ReplayError>>()
    })?;
    charts.sort_by(|left, right| left.chart.cmp(&right.chart));
    Ok(charts)
}

fn resolve_options(options: &Options) -> Result<Options, ReplayError> {
    let mut frozen = options.clone();
    if let Baseline::Git(revision) = &options.baseline {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--verify", &format!("{revision}^{{commit}}")])
            .current_dir(test_util::workspace_root())
            .output()
            .map_err(|source| ReplayError::Io {
                path: "git".into(),
                source,
            })?;
        if !output.status.success() {
            return Err(ReplayError::Git {
                fixture: revision.clone(),
                message: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        frozen.baseline = Baseline::Git(String::from_utf8_lossy(&output.stdout).trim().into());
    }
    Ok(frozen)
}

fn add_counts(total: &mut Counts, counts: &Counts) {
    let Counts {
        documents,
        fa_closed,
        fa_reopened,
        new_fr_candidates,
        unchanged,
        other_changes,
    } = counts;
    total.documents += documents;
    total.fa_closed += fa_closed;
    total.fa_reopened += fa_reopened;
    total.new_fr_candidates += new_fr_candidates;
    total.unchanged += unchanged;
    total.other_changes += other_changes;
}

#[tracing::instrument(target = "helm_schema::corpus_replay", skip_all, fields(chart = entry.key))]
fn replay_chart(
    options: &Options,
    entry: &manifest::ManifestEntry,
    baseline: Option<&manifest::Manifest>,
    classes: &evidence::ClassIndex,
) -> Result<ChartReport, ReplayError> {
    let chart = entry
        .key
        .strip_prefix("chart/")
        .ok_or_else(|| ReplayError::Input(entry.key.clone()))?;
    let fixture = entry
        .fixture
        .as_ref()
        .ok_or_else(|| ReplayError::Input(entry.key.clone()))?;
    let before = match &options.baseline {
        Baseline::Dump(path) => {
            let previous = baseline
                .and_then(|manifest| manifest.artifacts.iter().find(|row| row.key == entry.key))
                .ok_or_else(|| ReplayError::Input(format!("baseline missing {}", entry.key)))?;
            if previous.recipe != entry.recipe || previous.inputs_sha256 != entry.inputs_sha256 {
                return Err(ReplayError::Input(format!(
                    "baseline recipe inputs differ for {chart}"
                )));
            }
            dump::artifact(path, previous)?
        }
        Baseline::Git(revision) => {
            let output = std::process::Command::new("git")
                .args(["show", &format!("{revision}:{fixture}")])
                .current_dir(test_util::workspace_root())
                .output()
                .map_err(|source| ReplayError::Io {
                    path: "git".into(),
                    source,
                })?;
            if !output.status.success() {
                return Err(ReplayError::Git {
                    fixture: fixture.clone(),
                    message: String::from_utf8_lossy(&output.stderr).into_owned(),
                });
            }
            output.stdout
        }
    };
    let after = dump::artifact(&options.candidate, entry)?;
    let mut before_schema = parse(&before, Path::new(fixture))?;
    helm_schema_core::normalize_schema_pattern_dialects(&mut before_schema);
    let mut report = replay_saved(&SavedInputs {
        chart,
        baseline: &before_schema,
        candidate: &parse(&after, Path::new(&entry.file))?,
        evidence: &options.evidence,
        classes,
    })?;
    report.baseline_sha256 = sha256_hex(&before);
    report.candidate_sha256 = sha256_hex(&after);
    Ok(report)
}

/// Schemas and saved documents for a single chart, independent of dump provenance.
pub struct SavedInputs<'a> {
    /// Corpus chart name.
    pub chart: &'a str,
    /// Baseline schema, already dialect-normalized when historical.
    pub baseline: &'a Value,
    /// Candidate schema.
    pub candidate: &'a Value,
    /// Saved preview evidence root.
    pub evidence: &'a Path,
    /// Source paths to census target/document memberships.
    pub classes: &'a BTreeMap<String, Vec<String>>,
}

/// Replays one chart's saved documents, compiling each schema once.
///
/// This lower-level entry point does not verify producer provenance; [`run`] does.
///
/// # Errors
///
/// Returns an error for invalid schemas, missing documents, or malformed saved evidence.
pub fn replay_saved(inputs: &SavedInputs<'_>) -> Result<ChartReport, ReplayError> {
    let SavedInputs {
        chart,
        baseline,
        candidate,
        evidence,
        classes,
    } = inputs;
    let before_validator = compile(baseline, "baseline")?;
    let identical = baseline == candidate;
    let candidate_validator = if identical {
        None
    } else {
        Some(compile(candidate, "candidate")?)
    };
    let after_validator = candidate_validator.as_ref().unwrap_or(&before_validator);
    let mut report = ChartReport {
        chart: (*chart).into(),
        baseline_sha256: String::new(),
        candidate_sha256: String::new(),
        counts: Counts::default(),
        classes: BTreeMap::new(),
        distinct_documents: 0,
        unresolved_index_references: Vec::new(),
        changes: Vec::new(),
    };
    let mut cache = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let evidence::Evidence {
        documents,
        unresolved_index_references,
    } = evidence::documents(evidence, chart)?;
    report.unresolved_index_references = unresolved_index_references;
    for document in documents {
        let evidence::Document {
            source,
            value,
            oracle,
        } = document;
        seen.insert(source.clone());
        let bytes = serde_json::to_vec(value.as_ref()).map_err(|err| ReplayError::Json {
            path: source_path(&source),
            source: err,
        })?;
        let digest = sha256_hex(&bytes);
        let verdicts = cache.entry(digest.clone()).or_insert_with(|| {
            let before = before_validator.is_valid(&value);
            let after = if identical {
                before
            } else {
                after_validator.is_valid(&value)
            };
            Verdicts { before, after }
        });
        let Verdicts { before, after } = *verdicts;
        let mut row = Record {
            source,
            document_sha256: digest,
            before,
            after,
            oracle,
            classes: Vec::new(),
        };
        row.classes = classes
            .get(&row.source)
            .cloned()
            .unwrap_or_else(|| vec!["unclassified".into()]);
        report.counts.record(&row);
        for class in &row.classes {
            report
                .classes
                .entry(class.clone())
                .or_default()
                .record(&row);
        }
        if before != after {
            report.changes.push(row);
        }
    }
    let chart_root = evidence.join("cases").join(format!("chart-{chart}"));
    if let Some(missing) = classes
        .keys()
        .find(|source| source_path(source).starts_with(&chart_root) && !seen.contains(*source))
    {
        return Err(ReplayError::Input(format!(
            "census document not replayed: {missing}"
        )));
    }
    report.distinct_documents = cache.len();
    Ok(report)
}

fn source_path(source: &str) -> PathBuf {
    source.split('#').next().unwrap_or(source).into()
}

impl Report {
    /// Renders a compact Markdown table using the same counts as the JSON report.
    #[must_use]
    pub fn markdown(&self) -> String {
        let mut rows = vec![String::from(indoc::indoc! {"
            | Chart / class | Documents | FA closed | FA reopened | New FR candidates | Unchanged | Other changes |
            |---|---:|---:|---:|---:|---:|---:|
        "}.trim_end())];
        let mut row = |name: &str, counts: &Counts| {
            let Counts {
                documents,
                fa_closed,
                fa_reopened,
                new_fr_candidates,
                unchanged,
                other_changes,
            } = counts;
            rows.push(format!("| {name} | {documents} | {fa_closed} | {fa_reopened} | {new_fr_candidates} | {unchanged} | {other_changes} |"));
        };
        row("**Total**", &self.counts);
        for chart in &self.charts {
            row(&chart.chart, &chart.counts);
            for (class, counts) in &chart.classes {
                row(&format!("{} / {class}", chart.chart), counts);
            }
        }
        for (class, counts) in &self.classes {
            row(&format!("Class {class}"), counts);
        }
        let missing: usize = self
            .charts
            .iter()
            .map(|chart| chart.unresolved_index_references.len())
            .sum();
        rows.push(String::new());
        rows.push(format!("Unresolved index references: {missing}. These are excluded from document and verdict counts; see the JSON report."));
        rows.join("\n") + "\n"
    }
}
