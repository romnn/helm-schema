mod options;

use serde_json::Value;

use crate::error::CliResult;
use crate::session::AnalysisSession;

pub use options::GenerateOptions;
pub use options::GeneratedSchema;
pub use options::ResolvedContract;

/// Generate a values JSON schema for a full Helm chart.
///
/// This is the one remaining free-function compatibility shim. The session
/// API is the real public seam; callers that need diagnostics, staged
/// artifacts, or reuse should use [`AnalysisSession`] directly.
///
/// # Errors
///
/// Returns an error if charts cannot be discovered, files cannot be read, or
/// templates/values cannot be parsed.
pub fn generate_values_schema_for_chart(opts: &GenerateOptions) -> CliResult<Value> {
    Ok(AnalysisSession::new(opts.clone())
        .generated_schema()?
        .schema)
}
