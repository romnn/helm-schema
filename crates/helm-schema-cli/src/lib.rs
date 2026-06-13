mod analysis;
mod chart;
pub mod cli;
mod diag_emit;
mod error;
pub mod flatten;
mod output_pipeline;
mod provider_builder;
mod required_inference;
pub mod schema_override;

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_k8s::DiagnosticSink;
use serde_json::Value;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;
use vfs::VfsPath;

use crate::analysis::{ChartAnalysis, analyze_charts};
use crate::error::CliResult;
use crate::output_pipeline::OutputPipelineOptions;

pub use cli::Cli;
pub use error::CliError;
pub use provider_builder::ProviderOptions;

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    pub chart_dir: VfsPath,
    pub include_tests: bool,
    pub include_subchart_values: bool,
    pub values_files: Vec<PathBuf>,
    pub infer_required: bool,
    pub provider: ProviderOptions,
}

#[derive(Debug, Clone)]
struct GeneratedSchema {
    schema: Value,
    subchart_value_prefixes: Vec<Vec<String>>,
}

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
    } = generate_values_schema_for_chart_with_diagnostics_inner(&opts, Some(&diagnostics))?;
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

/// Generate a values JSON schema for a full Helm chart.
///
/// # Errors
///
/// Returns an error if charts cannot be discovered, files cannot be read, or
/// templates/values cannot be parsed.
pub fn generate_values_schema_for_chart(opts: &GenerateOptions) -> CliResult<Value> {
    generate_values_schema_for_chart_with_diagnostics(opts, None)
}

/// Generate a values JSON schema for a full Helm chart, collecting diagnostics.
///
/// # Errors
///
/// Returns an error if charts cannot be discovered, files cannot be read, or
/// templates/values cannot be parsed.
pub fn generate_values_schema_for_chart_with_diagnostics(
    opts: &GenerateOptions,
    diagnostic_sink: Option<&DiagnosticSink>,
) -> CliResult<Value> {
    let generated = generate_values_schema_for_chart_with_diagnostics_inner(opts, diagnostic_sink)?;
    Ok(generated.schema)
}

#[tracing::instrument(skip_all)]
fn generate_values_schema_for_chart_with_diagnostics_inner(
    opts: &GenerateOptions,
    diagnostic_sink: Option<&DiagnosticSink>,
) -> CliResult<GeneratedSchema> {
    let discovery = chart::discover_chart_contexts(&opts.chart_dir)?;
    let charts = &discovery.charts;

    let defines = chart::build_define_index(charts, opts.include_tests)?;

    let values_yaml = chart::build_composed_values_yaml(charts, opts.include_subchart_values)?;
    let values_descriptions = chart::build_composed_values_descriptions(
        charts,
        opts.include_subchart_values,
        &opts.values_files,
    )?;

    let ChartAnalysis {
        contract_projection,
        chart_facts,
        type_hints,
        call_graph,
        local_schema_universe,
    } = analyze_charts(charts, &defines, opts.include_tests, values_yaml.as_deref())?;

    let mut provider_options = opts.provider.clone();
    provider_options.local_schema_universe = local_schema_universe;
    let provider = provider_builder::build_provider(&provider_options, diagnostic_sink);

    let mut schema = generate_values_schema(
        ValuesSchemaInput::new(&contract_projection, &provider)
            .with_values_yaml(values_yaml.as_deref())
            .with_type_hints(&type_hints)
            .with_chart_facts(&chart_facts)
            .with_values_descriptions(&values_descriptions),
    );

    if opts.infer_required {
        required_inference::apply(
            &mut schema,
            &contract_projection,
            values_yaml.as_deref(),
            charts,
            &call_graph,
        );
    }

    Ok(GeneratedSchema {
        schema,
        subchart_value_prefixes: charts
            .iter()
            .filter(|chart| !chart.values_prefix.is_empty())
            .map(|chart| chart.values_prefix.clone())
            .collect(),
    })
}
