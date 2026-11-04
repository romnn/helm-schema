use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

use super::{MissingReference, Oracle, ReplayError, json, read};

pub(super) type ClassIndex = BTreeMap<String, Vec<String>>;

#[derive(Deserialize)]
pub(super) struct Census {
    documents: Vec<CensusDocument>,
}

#[derive(Deserialize)]
struct CensusDocument {
    document_path: PathBuf,
    document_field: Option<String>,
    class: String,
}

impl Census {
    pub(super) fn index(self, root: &Path) -> Result<ClassIndex, ReplayError> {
        let mut index: ClassIndex = BTreeMap::new();
        for CensusDocument {
            document_path,
            document_field,
            class,
        } in self.documents
        {
            let path = saved_path(root, root, &document_path)?;
            let source = match document_field {
                Some(field) => format!("{}#/{field}", path.display()),
                None => path.display().to_string(),
            };
            index.entry(source).or_default().push(class);
        }
        Ok(index)
    }
}

pub(super) struct Document {
    pub source: String,
    pub value: Arc<Value>,
    pub oracle: Oracle,
}

pub(super) struct Evidence {
    pub documents: Vec<Document>,
    pub unresolved_index_references: Vec<MissingReference>,
}

/// One immutable chart's evidence, memoized by exact file path within this replay only.
#[derive(Default)]
struct Reader {
    documents: BTreeMap<PathBuf, Arc<Value>>,
    statuses: BTreeMap<PathBuf, Option<bool>>,
}

impl Reader {
    fn document(&mut self, path: &Path) -> Result<Arc<Value>, ReplayError> {
        if let Some(value) = self.documents.get(path) {
            return Ok(Arc::clone(value));
        }
        let value = Arc::new(json(path)?);
        self.documents.insert(path.into(), Arc::clone(&value));
        Ok(value)
    }

    fn status(&mut self, path: &Path) -> Result<Option<bool>, ReplayError> {
        if let Some(outcome) = self.statuses.get(path) {
            return Ok(*outcome);
        }
        let outcome = status(path)?;
        self.statuses.insert(path.into(), outcome);
        Ok(outcome)
    }
}

fn optional_json(path: &Path) -> Result<Value, ReplayError> {
    match read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|source| ReplayError::Json {
            path: path.into(),
            source,
        }),
        Err(ReplayError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok(Value::Null)
        }
        Err(err) => Err(err),
    }
}

fn saved_path(root: &Path, directory: &Path, path: &Path) -> Result<PathBuf, ReplayError> {
    if path.is_relative() {
        return Ok(directory.join(path));
    }
    let parts: Vec<_> = path.components().collect();
    if let Some(index) = parts.iter().position(|part| part.as_os_str() == "cases") {
        return Ok(root.join(parts.iter().skip(index).collect::<PathBuf>()));
    }
    // Preserved verdicts can still name their old checkout's target/evidence root.
    // The owning probe and adjudication directory names retain the portable suffix.
    for ancestor in directory.ancestors() {
        let Some(name) = ancestor.file_name() else {
            continue;
        };
        if let Some(index) = parts.iter().rposition(|part| part.as_os_str() == name) {
            let rebased = ancestor.join(parts.iter().skip(index + 1).collect::<PathBuf>());
            if rebased.exists() {
                return Ok(rebased);
            }
        }
    }
    if let Some(index) = parts
        .iter()
        .position(|part| part.as_os_str() == "helm-schema")
    {
        let cases = root.join("cases");
        for ancestor in directory.ancestors() {
            if ancestor.parent() == Some(cases.as_path()) {
                let rebased = ancestor.join(parts.iter().skip(index + 1).collect::<PathBuf>());
                if rebased.exists() {
                    return Ok(rebased);
                }
            }
        }
    }
    if path.exists() {
        return Ok(path.into());
    }
    Err(ReplayError::Input(format!(
        "cannot locate saved evidence {} from {}",
        path.display(),
        directory.display()
    )))
}

fn files(directory: &Path, found: &mut BTreeSet<PathBuf>) -> Result<(), ReplayError> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(directory).map_err(|source| ReplayError::Io {
        path: directory.into(),
        source,
    })? {
        let entry = entry.map_err(|source| ReplayError::Io {
            path: directory.into(),
            source,
        })?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|source| ReplayError::Io {
            path: path.clone(),
            source,
        })?;
        if kind.is_dir() {
            // Preserved charts and inputs are render inputs, not saved acceptance documents.
            if entry.file_name() != "charts" && entry.file_name() != "inputs" {
                files(&path, found)?;
            }
        } else if kind.is_file()
            && matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("coalesced.json" | "schema-verdicts.json")
            )
        {
            found.insert(
                path.parent()
                    .ok_or_else(|| ReplayError::Input(path.display().to_string()))?
                    .into(),
            );
        }
    }
    Ok(())
}

pub(super) fn documents(root: &Path, chart: &str) -> Result<Evidence, ReplayError> {
    let mut directories = BTreeSet::new();
    files(
        &root.join("cases").join(format!("chart-{chart}")),
        &mut directories,
    )?;
    let mut missing = BTreeSet::new();
    let indexed = rejection_index(root, chart, &mut directories, &mut missing)?;
    let mut result = BTreeMap::new();
    let mut evidence_reader = Reader::default();
    for directory in directories {
        let verdict_path = directory.join("schema-verdicts.json");
        let verdict = optional_json(&verdict_path)?;
        let oracle = oracle(
            root,
            &directory,
            &verdict,
            indexed.get(&directory).map(String::as_str),
            &mut evidence_reader,
        )?;
        let path = directory.join("coalesced.json");
        let has_coalesced = evidence_reader.documents.contains_key(&path) || path.exists();
        if has_coalesced {
            insert(
                &mut result,
                Document {
                    source: path.display().to_string(),
                    value: evidence_reader.document(&path)?,
                    oracle,
                },
            )?;
        }
        if let Some(value) = verdict.get("document") {
            let source = format!("{}#/document", verdict_path.display());
            insert(
                &mut result,
                Document {
                    source,
                    value: Arc::new(value.clone()),
                    oracle,
                },
            )?;
        }
        if !has_coalesced && verdict.get("document").is_none() {
            return Err(ReplayError::Input(format!(
                "saved probe has no document: {}",
                directory.display()
            )));
        }
        for control in verdict
            .get("controls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let render = control
                .get("render_evidence")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ReplayError::Input(format!(
                        "control has no render evidence: {}",
                        verdict_path.display()
                    ))
                })?;
            let path = saved_path(root, &directory, Path::new(render))?.join("coalesced.json");
            let value = evidence_reader.document(&path)?;
            if control.get("render_success").and_then(Value::as_bool) == Some(true)
                && control.get("schema_free_lint").and_then(Value::as_str) == Some("Pass")
            {
                let source = path.display().to_string();
                insert(
                    &mut result,
                    Document {
                        source,
                        value,
                        oracle: Oracle::Valid,
                    },
                )?;
            }
        }
    }
    Ok(Evidence {
        documents: result.into_values().collect(),
        unresolved_index_references: missing.into_iter().collect(),
    })
}

fn rejection_index(
    root: &Path,
    chart: &str,
    directories: &mut BTreeSet<PathBuf>,
    missing: &mut BTreeSet<MissingReference>,
) -> Result<BTreeMap<PathBuf, String>, ReplayError> {
    let mut indexed = BTreeMap::new();
    let chart_root = root.join("cases").join(format!("chart-{chart}"));
    for suffix in [
        "new-false-acceptances",
        "false-rejections",
        "new-undecided",
        "unlisted-hard-failures",
    ] {
        let index_path = root.join(format!("chart-{chart}-{suffix}.json"));
        if index_path.exists() {
            let rows: Vec<Value> = json(&index_path)?;
            for row in rows {
                if let Some(reference) = row.get("evidence").and_then(Value::as_str) {
                    let recorded = Path::new(reference);
                    let owner = if recorded.is_relative() {
                        root
                    } else {
                        &chart_root
                    };
                    let resolved = match saved_path(root, owner, recorded) {
                        Ok(resolved) => Some(resolved),
                        Err(ReplayError::Input(_)) => None,
                        Err(err) => return Err(err),
                    };
                    let Some(path) = resolved.filter(|path| path.is_dir()) else {
                        missing.insert(MissingReference {
                            index: index_path.display().to_string(),
                            reference: reference.to_owned(),
                        });
                        continue;
                    };
                    directories.insert(path.clone());
                    if let Some(rejection) = row.get("rejection").and_then(Value::as_str) {
                        indexed.insert(path, rejection.to_owned());
                    }
                }
            }
        }
    }
    Ok(indexed)
}

fn insert(
    documents: &mut BTreeMap<String, Document>,
    document: Document,
) -> Result<(), ReplayError> {
    if let Some(previous) = documents.get_mut(&document.source) {
        if !Arc::ptr_eq(&previous.value, &document.value) && previous.value != document.value {
            return Err(ReplayError::Input(format!(
                "conflicting saved documents at {}",
                document.source
            )));
        }
        // A later scan must not erase a control's independently saved successful result.
        if document.oracle == Oracle::Rejects || previous.oracle == Oracle::Unknown {
            previous.oracle = document.oracle;
        }
    } else {
        documents.insert(document.source.clone(), document);
    }
    Ok(())
}

fn status(path: &Path) -> Result<Option<bool>, ReplayError> {
    let bytes = match read(path) {
        Ok(bytes) => bytes,
        Err(ReplayError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(err) => return Err(err),
    };
    let text = String::from_utf8_lossy(&bytes);
    // Saved statuses are either an exit integer or a labelled exit integer.
    let exit = text
        .trim()
        .rsplit(':')
        .next()
        .and_then(|exit| exit.trim().parse::<i32>().ok());
    Ok(exit.map(|exit| exit != 0))
}

fn oracle(
    root: &Path,
    directory: &Path,
    verdict: &Value,
    indexed: Option<&str>,
    evidence_reader: &mut Reader,
) -> Result<Oracle, ReplayError> {
    let mut outcomes = Vec::new();
    if matches!(
        indexed,
        Some("HelmAborts" | "LintRejects" | "KubernetesRejects")
    ) {
        return Ok(Oracle::Rejects);
    }
    if let Some(exit) = verdict.get("helm_exit").and_then(Value::as_i64) {
        outcomes.push(exit != 0);
    }
    for name in ["render.status", "lint-schema-free.status"] {
        outcomes.extend(evidence_reader.status(&directory.join(name))?);
    }
    if let Some(path) = verdict.get("render_evidence").and_then(Value::as_str) {
        outcomes.extend(
            evidence_reader
                .status(&saved_path(root, directory, Path::new(path))?.join("render.status"))?,
        );
    }
    let has_errors = |value: &Value| {
        value
            .get("new_kubernetes_errors")
            .and_then(Value::as_array)
            .is_some_and(|errors| !errors.is_empty())
    };
    if outcomes.contains(&true)
        || has_errors(verdict)
        || verdict
            .get("kubernetes_contexts")
            .and_then(Value::as_array)
            .is_some_and(|contexts| contexts.iter().any(has_errors))
    {
        Ok(Oracle::Rejects)
    } else if outcomes.contains(&false) {
        Ok(Oracle::Valid)
    } else {
        Ok(Oracle::Unknown)
    }
}
