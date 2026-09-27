//! Command-line argument model and invocation policy for `helm-schema`.

/// Typed command-line arguments and option validation.
pub mod cli;
mod config;
mod diag_emit;

use std::collections::BTreeMap;
use std::io::{BufWriter, Write};
use std::path::Path;

use helm_schema::chart_source::RootChartSource;
use helm_schema::diagnostics::DiagnosticSink;
use helm_schema::output::{
    FetchPolicy, HELM_MAX_CHART_FILE_BYTES, JsonOutputFormat, LoadBudget, PolicyInputOptions,
    shorten_definition_names, write_schema_json_without_metrics,
};
use helm_schema::{AnalysisSession, EngineResult};
use serde_json::Value;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;

pub use cli::Cli;
pub use helm_schema::generation::{GenerateOptions, SchemaProfile};
pub use helm_schema::provider::ProviderOptions;
pub use helm_schema::{CliError, flatten, schema_override};

/// Run the CLI.
///
/// # Errors
///
/// Returns an error if chart discovery fails, a template/values file cannot be
/// read/parsed, the schema cannot be generated, or output cannot be written.
pub fn run(cli: Cli) -> EngineResult<()> {
    if let Some(cli::Command::Shorten(args)) = &cli.command {
        return shorten(args, cli.diag.diag_format);
    }
    let trace_output = cli.perf.trace_output.clone();
    if let Some(trace_output) = trace_output {
        let trace_file = create_output_file(&trace_output)?;
        let perfetto_layer =
            tracing_perfetto::PerfettoLayer::new(std::sync::Mutex::new(trace_file))
                .with_debug_annotations(true)
                .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                    metadata.target().starts_with("helm_schema")
                        || metadata
                            .target()
                            .starts_with("helm_schema_json_schema_minify")
                }));
        let subscriber = tracing_subscriber::registry().with(perfetto_layer);
        let dispatch = tracing::Dispatch::new(subscriber);
        return tracing::dispatcher::with_default(&dispatch, || run_inner(cli));
    }

    run_inner(cli)
}

fn run_inner(cli: Cli) -> EngineResult<()> {
    let Some(chart_dir_arg) = cli.chart_dir.clone() else {
        return Err(CliError::CliValidation(
            "a CHART_DIR to analyze is required".to_string(),
        ));
    };
    let run_span = tracing::info_span!(
        "helm_schema_run",
        chart_dir = %chart_dir_arg.display()
    );
    let _entered = run_span.enter();

    let root_source = RootChartSource::open(&chart_dir_arg, LoadBudget::default())?;
    let effective_config = config::resolve(
        &root_source,
        &chart_dir_arg,
        cli.config.as_deref(),
        cli.no_config,
        cli.profile,
        cli.emission,
    )?;

    let diagnostics = DiagnosticSink::new();
    if !effective_config.file_weakening.is_empty() {
        diagnostics.push(
            helm_schema::diagnostics::Diagnostic::DiscoveredConfigWeakensEmission {
                disabled_knobs: effective_config
                    .file_weakening
                    .iter()
                    .map(|knob| (*knob).to_string())
                    .collect(),
                explicit: effective_config.file_weakening_is_explicit,
            },
        );
    }
    if cli.print_effective_config {
        let stdout = std::io::stdout();
        let mut out = BufWriter::new(stdout.lock());
        out.write_all(effective_config.to_yaml()?.as_bytes())?;
        out.flush()?;
        diag_emit::emit_to_stderr(&diagnostics, cli.diag.diag_format);
        return Ok(());
    }

    let generated = (|| {
        cli.crd.validate().map_err(CliError::CliValidation)?;
        let fallback_window = cli
            .k8s
            .resolved_fallback_window()
            .map_err(CliError::CliValidation)?;
        let chart_dir = root_source.into_chart_dir();
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
            local_schema_universe: helm_schema::provider::LocalSchemaUniverse::default(),
            crd_cache_record_source: cli.crd.crd_cache_record_source,
            api_version_guess: cli.inference.enabled(),
        };
        let opts = GenerateOptions {
            chart_dir,
            include_tests: !cli.chart.exclude_tests,
            include_subchart_values: !cli.chart.no_subchart_values,
            values_files: cli.chart.values_files.clone(),
            infer_required: cli.chart.infer_required,
            emission: effective_config.selection,
            provider: provider_options,
        };
        let session = AnalysisSession::with_diagnostics(opts, diagnostics.clone());
        let policy_input_options = PolicyInputOptions {
            fetch_policy: FetchPolicy::input_assembly(!cli.k8s.offline),
            load_budget: LoadBudget::default(),
        };
        session.emit_with_policy_paths(
            &cli.override_schema,
            policy_input_options,
            cli.output.emit_request(),
        )
    })();
    diag_emit::emit_to_stderr(&diagnostics, cli.diag.diag_format);
    let schema = generated?;

    let json_format = cli.output.json_format();
    let output = cli.output.output;
    if !cli.output.shorten_defs {
        let bytes = write_schema(output.as_deref(), &schema, json_format)?;
        warn_over_helm_limit(bytes, false, cli.diag.diag_format);
        return Ok(());
    }
    let shortened = shorten_definition_names(&schema);
    let bytes = write_schema(output.as_deref(), &shortened.schema, json_format)?;
    if let Some(path) = &cli.output.defs_map {
        write_definition_map(path, &shortened.readable_names)?;
    }
    warn_over_helm_limit(bytes, true, cli.diag.diag_format);
    Ok(())
}

/// `helm-schema shorten`: the explicit rename of a readable schema's `$defs`
/// to short keys, and the map back.
fn shorten(args: &cli::ShortenArgs, diag_format: cli::DiagFormat) -> EngineResult<()> {
    let bytes = std::fs::read(&args.input).map_err(|source| CliError::ReadSchema {
        path: args.input.clone(),
        source,
    })?;
    let schema: Value = serde_json::from_slice(&bytes).map_err(|source| CliError::ParseSchema {
        path: args.input.clone(),
        source,
    })?;
    let shortened = shorten_definition_names(&schema);
    let written = write_schema(Some(&args.output), &shortened.schema, args.json_format())?;
    if let Some(path) = &args.map {
        write_definition_map(path, &shortened.readable_names)?;
    }
    warn_over_helm_limit(written, true, diag_format);
    Ok(())
}

/// Writes `schema` to `path`, or to standard output; returns the bytes written.
fn write_schema(
    path: Option<&Path>,
    schema: &Value,
    format: JsonOutputFormat,
) -> EngineResult<usize> {
    let Some(path) = path else {
        let stdout = std::io::stdout();
        let mut out = BufWriter::new(stdout.lock());
        let written = write_schema_json_without_metrics(&mut out, schema, format)?;
        out.flush()?;
        return Ok(written);
    };
    let mut out = BufWriter::new(create_output_file(path)?);
    let written = write_schema_json_without_metrics(&mut out, schema, format)
        .map_err(|err| write_output_error_with_path(err, path))?;
    out.flush().map_err(|err| CliError::WriteOutput {
        path: path.to_path_buf(),
        source: err,
    })?;
    Ok(written)
}

/// Writes the map from each short `$defs` key to its readable name.
fn write_definition_map(
    path: &Path,
    readable_names: &BTreeMap<String, String>,
) -> EngineResult<()> {
    let mut bytes = serde_json::to_vec_pretty(readable_names)?;
    bytes.push(b'\n');
    let mut out = create_output_file(path)?;
    out.write_all(&bytes)
        .map_err(|source| CliError::WriteOutput {
            path: path.to_path_buf(),
            source,
        })
}

/// Warns when Helm would refuse a schema of `bytes` bytes.
fn warn_over_helm_limit(bytes: usize, shortened: bool, format: cli::DiagFormat) {
    if bytes <= HELM_MAX_CHART_FILE_BYTES {
        return;
    }
    let diagnostics = DiagnosticSink::new();
    diagnostics.push(
        helm_schema::diagnostics::Diagnostic::SchemaExceedsHelmFileLimit {
            bytes,
            limit: HELM_MAX_CHART_FILE_BYTES,
            shortened,
        },
    );
    diag_emit::emit_to_stderr(&diagnostics, format);
}

fn create_output_file(path: &Path) -> EngineResult<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| CliError::CreateOutputDir {
            path: parent.to_path_buf(),
            source: err,
        })?;
    }
    std::fs::File::create(path).map_err(|err| CliError::WriteOutput {
        path: path.to_path_buf(),
        source: err,
    })
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
