mod canonicalise;
#[expect(
    clippy::module_inception,
    reason = "the private diagnostic model shares the public module's domain name"
)]
mod diagnostic;
mod format_json;
mod format_text;
mod sink;

pub use diagnostic::{Diagnostic, DiagnosticKey};
pub use format_json::format_diagnostic_json;
pub use format_text::format_diagnostic_text;
pub use sink::DiagnosticSink;
