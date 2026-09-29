//! Explain the generation decisions for the given values paths:
//! `cargo run --example explain_values -- <chart-dir> <values-path>...`
//!
//! Reports go to stdout: text trees by default, or with `EXPLAIN_JSON=1` one
//! JSON document (the report for one path, an array of reports for several).
//! Diagnostics go to stderr: the terminal clauses touching the paths, and the
//! whole contract IR with `RAW_CONTRACT=1`.
//!
//! Provider schemas come from `.cache/` offline, or from the provider bundle
//! named by `PROVIDER_BUNDLE` (for example `testdata/provider-bundle`).
use std::io::Write;
use std::path::PathBuf;

use helm_schema::explain::ExplainFormat;
use helm_schema::generation::SchemaProfile;
use helm_schema::provider::ProviderOptions;
use helm_schema::{AnalysisSession, GenerateOptions};
use vfs::VfsPath;

/// The example's environment switches.
pub struct ExplainOptions {
    /// `EXPLAIN_JSON`: JSON reports instead of text trees.
    pub json: bool,
    /// `RAW_CONTRACT`: dump the contract IR to stderr.
    pub raw_contract: bool,
    /// `PROVIDER_BUNDLE`: the provider schema bundle directory.
    pub provider_bundle: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = ExplainOptions {
        json: std::env::var_os("EXPLAIN_JSON").is_some(),
        raw_contract: std::env::var_os("RAW_CONTRACT").is_some(),
        provider_bundle: std::env::var_os("PROVIDER_BUNDLE")
            .map_or_else(|| PathBuf::from(".cache"), PathBuf::from),
    };
    run(
        &args,
        &options,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
}

/// Explains `<chart-dir> <values-path>...` into `stdout`, with diagnostics on
/// `stderr`.
///
/// # Errors
///
/// Returns an error when the arguments are incomplete, analysis or generation
/// fails, or an output cannot be written.
pub fn run(
    args: &[String],
    options: &ExplainOptions,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let (chart, requested) = args.split_first().ok_or("chart path")?;
    let requested_paths: Vec<helm_schema_core::ValuesPath> = requested
        .iter()
        .map(|path| helm_schema_core::ValuesPath::parse(path))
        .collect();
    let format = if options.json {
        ExplainFormat::Json
    } else {
        ExplainFormat::Text
    };
    let bundle = &options.provider_bundle;
    let session = AnalysisSession::new(GenerateOptions {
        chart_dir: VfsPath::new(vfs::PhysicalFS::new(chart)),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::default().into(),
        authoring: helm_schema::generation::AuthoringPolicy::default(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(bundle.join("kubernetes-json-schema-cache")),
            allow_net: false,
            crd_catalog_cache_dir: Some(bundle.join("crds-catalog-cache")),
            disable_k8s_schemas: false,
            crd_override_dir: Some(bundle.join("crds-catalog-cache")),
            ..Default::default()
        },
    });
    if options.raw_contract {
        writeln!(stderr, "contract: {:#?}", session.analysis()?.contract)?;
    }
    let reports = requested
        .iter()
        .map(|path| session.explain_generation(path, format))
        .collect::<Result<Vec<_>, _>>()?;
    match (format, reports.as_slice()) {
        (ExplainFormat::Text, reports) => write!(stdout, "{}", reports.concat())?,
        (ExplainFormat::Json, [report]) => write!(stdout, "{report}")?,
        (ExplainFormat::Json, reports) => {
            let reports = reports
                .iter()
                .map(|report| serde_json::from_str::<serde_json::Value>(report))
                .collect::<Result<Vec<_>, _>>()?;
            writeln!(stdout, "{}", serde_json::to_string_pretty(&reports)?)?;
        }
    }
    let signals = session.contract_schema_signals()?;
    for clause in signals.terminal_clauses() {
        if clause.iter().any(|guard| {
            guard
                .value_paths()
                .iter()
                .any(|path| requested_paths.iter().any(|requested| requested == path))
        }) {
            writeln!(stderr, "terminal: {clause:#?}")?;
        }
    }
    Ok(())
}
