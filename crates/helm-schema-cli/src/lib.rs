mod analysis;
mod chart;
mod chart_files;
pub mod cli;
mod diag_emit;
mod error;
pub mod flatten;
mod generation;
mod output_pipeline;
mod provider_builder;
mod required_inference;
pub mod schema_override;

use std::io::{BufWriter, Write};
use std::path::Path;

use helm_schema_k8s::DiagnosticSink;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;
use vfs::VfsPath;

use crate::error::CliResult;
use crate::generation::{GeneratedSchema, generate_values_schema_for_chart_output};
use crate::output_pipeline::OutputPipelineOptions;

pub use cli::Cli;
pub use error::CliError;
pub use generation::{
    GenerateOptions, generate_values_schema_for_chart,
    generate_values_schema_for_chart_with_diagnostics,
};
pub use provider_builder::ProviderOptions;

/// Run the CLI.
///
/// # Errors
///
/// Returns an error if chart discovery fails, a template/values file cannot be
/// read/parsed, the schema cannot be generated, or output cannot be written.
pub fn run(cli: Cli) -> CliResult<()> {
    let trace_output = cli.perf.trace_output.clone();
    if let Some(trace_output) = trace_output {
        if let Some(parent) = trace_output.parent() {
            std::fs::create_dir_all(parent).map_err(|err| CliError::CreateOutputDir {
                path: parent.to_path_buf(),
                source: err,
            })?;
        }
        let trace_file =
            std::fs::File::create(&trace_output).map_err(|err| CliError::WriteOutput {
                path: trace_output.clone(),
                source: err,
            })?;
        let perfetto_layer =
            tracing_perfetto::PerfettoLayer::new(std::sync::Mutex::new(trace_file))
                .with_debug_annotations(true)
                .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                    metadata.target().starts_with("helm_schema")
                        || metadata.target().starts_with("json_schema_minify")
                }));
        let subscriber = tracing_subscriber::registry().with(perfetto_layer);
        let dispatch = tracing::Dispatch::new(subscriber);
        return tracing::dispatcher::with_default(&dispatch, || run_inner(cli));
    }

    run_inner(cli)
}

fn run_inner(cli: Cli) -> CliResult<()> {
    let run_span = tracing::info_span!(
        "helm_schema_run",
        chart_dir = %cli.chart_dir.display()
    );
    let _entered = run_span.enter();

    cli.crd.validate().map_err(CliError::CliValidation)?;
    let fallback_window = cli
        .k8s
        .resolved_fallback_window()
        .map_err(CliError::CliValidation)?;

    let chart_dir_str = cli.chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));

    let provider_options = ProviderOptions {
        k8s_versions: cli.k8s.k8s_version.clone(),
        k8s_version_fallback_window: fallback_window,
        k8s_schema_mirrors: cli.k8s.k8s_schema_mirror.clone(),
        k8s_schema_cache_dir: cli.k8s.k8s_schema_cache_dir.clone(),
        no_cache: cli.k8s.no_cache,
        allow_net: !cli.k8s.offline,
        disable_k8s_schemas: cli.k8s.no_k8s_schemas,
        crd_lookup_loose: matches!(cli.crd.lookup_mode(), cli::CrdVersionLookup::Loose),
        crd_catalog_mirrors: cli.crd.crd_catalog_mirror.clone(),
        crd_catalog_cache_dir: cli.crd.crd_catalog_cache_dir.clone(),
        crd_override_dir: cli.crd.crd_override_dir.clone(),
        local_schema_universe: Default::default(),
        crd_cache_record_source: cli.crd.crd_cache_record_source,
        api_version_guess: cli.inference.enabled(),
    };

    let opts = GenerateOptions {
        chart_dir,
        include_tests: !cli.chart.exclude_tests,
        include_subchart_values: !cli.chart.no_subchart_values,
        values_files: cli.chart.values_files.clone(),
        infer_required: cli.chart.infer_required,
        provider: provider_options,
    };

    let diagnostics = DiagnosticSink::new();
    let GeneratedSchema {
        mut schema,
        subchart_value_prefixes,
    } = generate_values_schema_for_chart_output(&opts, Some(&diagnostics))?;
    let output_options = OutputPipelineOptions {
        keep_refs: cli.output.keep_refs,
        allow_net: !cli.k8s.offline,
        strip_descriptions: cli.output.strip_descriptions,
        minimize: cli.output.minimize,
    };

    schema = output_pipeline::apply_schema_output_pipeline(
        schema,
        &cli.override_schema,
        &subchart_value_prefixes,
        &cli.chart_dir,
        &output_options,
    )?;

    diag_emit::emit_to_stderr(&diagnostics, cli.diag.diag_format);

    if let Some(path) = cli.output.output {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CliError::CreateOutputDir {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let file = std::fs::File::create(&path).map_err(|err| CliError::WriteOutput {
            path: path.clone(),
            source: err,
        })?;
        let mut out = BufWriter::new(file);
        output_pipeline::write_schema_json(&mut out, &schema, cli.output.compact)
            .map_err(|err| write_output_error_with_path(err, &path))?;
        out.flush().map_err(|err| CliError::WriteOutput {
            path: path.clone(),
            source: err,
        })?;
    } else {
        let stdout = std::io::stdout();
        let mut out = BufWriter::new(stdout.lock());
        output_pipeline::write_schema_json(&mut out, &schema, cli.output.compact)?;
        out.flush()?;
    }

    Ok(())
}

fn write_output_error_with_path(err: CliError, path: &Path) -> CliError {
    match err {
        CliError::Io(source) => CliError::WriteOutput {
            path: path.to_path_buf(),
            source,
        },
        err => err,
    }
}
