use super::diagnostic::Diagnostic;

/// Serialize a [`Diagnostic`] to a single-line JSON string. Tagged
/// discriminated union (`#[serde(tag = "type")]`) keeps the shape
/// stable across versions. Returns `None` only on JSON serialization
/// failure, which should never happen for the variants we own.
#[must_use]
pub fn format_diagnostic_json(diagnostic: &Diagnostic) -> Option<String> {
    serde_json::to_string(diagnostic).ok()
}
