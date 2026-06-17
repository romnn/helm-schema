use helm_schema_k8s::DiagnosticSink;
use serde_json::Value;

use crate::error::CliResult;
use crate::generation::options::{GenerateOptions, GeneratedSchema};
use crate::session::{PreparedSession, generate_schema_from_prepared};

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
pub fn generate_values_schema_for_chart_output(
    opts: &GenerateOptions,
    diagnostic_sink: Option<&DiagnosticSink>,
) -> CliResult<GeneratedSchema> {
    let prepared = PreparedSession::from_generate_options(opts)?;
    generate_schema_from_prepared(&prepared, opts, diagnostic_sink)
}
