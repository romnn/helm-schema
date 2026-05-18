use std::io::Write;

use helm_schema_k8s::{DiagnosticSink, format_diagnostic_json, format_diagnostic_text};

use crate::cli::DiagFormat;

/// Drain a [`DiagnosticSink`] to stderr in the configured format.
///
/// The post-parse JSON-mode contract: every emission goes through
/// here, so once `--diag-format=json` is selected, every stderr line
/// is a `Diagnostic` JSON object.
pub fn emit_to_stderr(sink: &DiagnosticSink, format: DiagFormat) {
    let mut stderr = std::io::stderr().lock();
    sink.for_each(|diagnostic| match format {
        DiagFormat::Text => {
            let _ = writeln!(stderr, "{}", format_diagnostic_text(diagnostic));
        }
        DiagFormat::Json => {
            if let Some(line) = format_diagnostic_json(diagnostic) {
                let _ = writeln!(stderr, "{line}");
            }
        }
    });
}
