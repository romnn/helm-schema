use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_k8s::DiagnosticSink;
use serde_json::Value;

use crate::analysis::{ChartAnalysis, analyze_charts};
use crate::chart;
use crate::error::CliResult;
use crate::generation::options::{GenerateOptions, GeneratedSchema};
use crate::provider_builder;
use crate::required_inference;

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
    let generated = generate_values_schema_for_chart_output(opts, diagnostic_sink)?;
    Ok(generated.schema)
}

#[tracing::instrument(skip_all)]
pub(crate) fn generate_values_schema_for_chart_output(
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
        contract_schema_signals,
        template_evidence,
        local_schema_universe,
    } = analyze_charts(charts, &defines, opts.include_tests, values_yaml.as_deref())?;

    let mut provider_options = opts.provider.clone();
    provider_options.local_schema_universe = local_schema_universe;
    let provider = provider_builder::build_provider(&provider_options, diagnostic_sink);

    let mut schema = generate_values_schema(
        ValuesSchemaInput::new(&contract_schema_signals, &provider)
            .with_values_yaml(values_yaml.as_deref())
            .with_type_hints(&template_evidence.type_hints)
            .with_values_descriptions(&values_descriptions),
    );

    if opts.infer_required {
        required_inference::apply(
            &mut schema,
            &contract_schema_signals.required_inference_signals,
            values_yaml.as_deref(),
            &template_evidence.default_fallback_paths,
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
