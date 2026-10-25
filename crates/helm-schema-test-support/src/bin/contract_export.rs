//! Exports every whole-chart recipe's public contract document beside the
//! schema the same analysis session emits.
//!
//! ```text
//! contract_export --out NEW_DIR [--jobs N]
//! ```
//!
//! For each registered chart recipe the exporter constructs one
//! `AnalysisSession` with the producer's options, writes
//! `<n>.contract.json` (the session's `contract_document()`) and
//! `<n>.schema.json` (the schema the producer would write) and lists both in
//! `export.json` with the build provenance. The differential gate requires
//! each schema to equal the producer's artifact byte for byte.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;

use helm_schema::AnalysisSession;
use helm_schema_test_support::generate::{generate_options, session_schema};
use helm_schema_test_support::manifest::BuildProvenance;
use helm_schema_test_support::registry::{self, GenerationRecipe};
use helm_schema_test_support::source_digest::sha256_hex;
use helm_schema_test_support::value_algebra_diff::revision::{
    ExportEntry, ExportFile, ExportManifest,
};

/// Why the export failed.
#[derive(Debug, thiserror::Error)]
enum ExportError {
    #[error("usage: contract_export --out NEW_DIR [--jobs N]")]
    Usage,
    #[error("{0}")]
    Stale(String),
    #[error("{key}: {message}")]
    Recipe { key: String, message: String },
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("contract_export: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), ExportError> {
    let mut out = None;
    let mut jobs = 3_usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--out", Some(value)) => out = Some(PathBuf::from(value)),
            ("--jobs", Some(value)) => jobs = value.parse().map_err(|_| ExportError::Usage)?,
            _ => return Err(ExportError::Usage),
        }
    }
    let out = out.ok_or(ExportError::Usage)?;
    let build = BuildProvenance::compiled();
    build
        .check_current()
        .map_err(|error| ExportError::Stale(error.to_string()))?;
    if out.exists() {
        return Err(ExportError::Io {
            path: out,
            source: std::io::Error::from(std::io::ErrorKind::AlreadyExists),
        });
    }
    std::fs::create_dir_all(&out).map_err(|source| ExportError::Io {
        path: out.clone(),
        source,
    })?;
    let recipes = registry::registry()
        .into_iter()
        .filter_map(|spec| match spec.recipe {
            GenerationRecipe::Chart(recipe) => Some((spec.id.key(), recipe)),
            GenerationRecipe::FinalPolicy(_)
            | GenerationRecipe::Template(_)
            | GenerationRecipe::Ir(_) => None,
        })
        .enumerate()
        .collect::<Vec<_>>();
    let queue = Mutex::new(recipes);
    let rows = Mutex::new(Vec::new());
    let failures = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| {
                while let Some((index, (key, recipe))) =
                    queue.lock().ok().and_then(|mut queue| queue.pop())
                {
                    match export_one(&out, index, &recipe) {
                        Ok((contract, schema)) => {
                            if let Ok(mut rows) = rows.lock() {
                                rows.push(ExportEntry {
                                    key,
                                    contract,
                                    schema,
                                });
                            }
                        }
                        Err(message) => {
                            if let Ok(mut failures) = failures.lock() {
                                failures.push(ExportError::Recipe { key, message });
                            }
                        }
                    }
                }
            });
        }
    });
    if let Some(failure) = failures
        .into_inner()
        .ok()
        .and_then(|mut failures| failures.pop())
    {
        return Err(failure);
    }
    let mut rows = rows.into_inner().unwrap_or_default();
    rows.sort_by(|left, right| left.key.cmp(&right.key));
    let manifest = ExportManifest {
        format_version: 1,
        build,
        entries: rows,
    };
    let path = out.join("export.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&manifest)?)
        .map_err(|source| ExportError::Io { path, source })
}

/// Writes one recipe's contract document and schema; returns both files.
fn export_one(
    out: &Path,
    index: usize,
    recipe: &registry::ChartRecipe,
) -> Result<(ExportFile, ExportFile), String> {
    let session = AnalysisSession::new(generate_options(recipe));
    let contract = session
        .contract_document()
        .map_err(|error| format!("contract document: {error}"))?;
    let contract = serde_json::to_vec_pretty(&contract).map_err(|error| error.to_string())?;
    let schema = session_schema(&session, recipe).map_err(|error| format!("{error:#}"))?;
    let mut schema = serde_json::to_vec_pretty(&schema).map_err(|error| error.to_string())?;
    schema.push(b'\n');
    let write = |suffix: &str, bytes: &[u8]| -> Result<ExportFile, String> {
        let file = format!("{index:04}.{suffix}.json");
        std::fs::write(out.join(&file), bytes).map_err(|error| format!("{file}: {error}"))?;
        Ok(ExportFile {
            file,
            sha256: sha256_hex(bytes),
        })
    };
    Ok((write("contract", &contract)?, write("schema", &schema)?))
}
