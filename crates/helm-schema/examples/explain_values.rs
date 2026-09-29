//! Explain the generation decisions for the given values paths, and which
//! terminal clauses touch them:
//! `cargo run --example explain_values -- <chart-dir> <values-path>...`
//! Set `EXPLAIN_JSON=1` for the JSON report instead of the text tree, and
//! `RAW_CONTRACT=1` to dump the whole contract IR first.
use std::path::PathBuf;

use helm_schema::explain::ExplainFormat;
use helm_schema::generation::SchemaProfile;
use helm_schema::provider::ProviderOptions;
use helm_schema::{AnalysisSession, GenerateOptions};
use vfs::VfsPath;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let chart = args.next().ok_or("chart path")?;
    let requested: Vec<String> = args.collect();
    let requested_paths: Vec<helm_schema_core::ValuesPath> = requested
        .iter()
        .map(|path| helm_schema_core::ValuesPath::parse(path))
        .collect();
    let format = if std::env::var_os("EXPLAIN_JSON").is_some() {
        ExplainFormat::Json
    } else {
        ExplainFormat::Text
    };
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart));
    let session = AnalysisSession::new(GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::default().into(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(PathBuf::from(".cache/kubernetes-json-schema-cache")),
            allow_net: false,
            crd_catalog_cache_dir: Some(PathBuf::from(".cache/crds-catalog-cache")),
            disable_k8s_schemas: false,
            crd_override_dir: Some(PathBuf::from(".cache/crds-catalog-cache")),
            ..Default::default()
        },
    });
    if std::env::var_os("RAW_CONTRACT").is_some() {
        println!("contract: {:#?}", session.analysis()?.contract);
    }
    for path in &requested {
        print!("{}", session.explain_generation(path, format)?);
    }
    let signals = session.contract_schema_signals()?;
    for clause in signals.terminal_clauses() {
        if clause.iter().any(|guard| {
            guard
                .value_paths()
                .iter()
                .any(|path| requested_paths.iter().any(|requested| requested == path))
        }) {
            println!("terminal: {clause:#?}");
        }
    }
    Ok(())
}
