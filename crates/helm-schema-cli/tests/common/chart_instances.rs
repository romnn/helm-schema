use color_eyre::eyre;
use serde_json::Value;

/// Compose a sparse values override over the chart defaults the way Helm
/// coalesces them, and return the document a schema validates.
pub fn with_override(chart_relative_path: &str, override_value: Value) -> eyre::Result<Value> {
    Ok(test_util::helm_values::coalesce_chart_values(
        &helm_schema_test_support::generate::chart_dir(chart_relative_path),
        override_value,
    )?)
}
