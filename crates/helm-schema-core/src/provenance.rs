use serde::{Deserialize, Serialize};

/// Byte span within a template source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Inclusive byte offset where the source range begins.
    pub start: usize,
    /// Exclusive byte offset where the source range ends.
    pub end: usize,
}

impl SourceSpan {
    /// Creates a half-open source range.
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Source provenance for one emitted contract fact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractProvenance {
    /// Chart-relative template file containing the evidence.
    pub template_path: String,
    /// Byte range of the expression that produced the evidence.
    pub span: SourceSpan,
    /// Named helpers traversed from the template to the expression.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub helper_chain: Vec<String>,
}

impl ContractProvenance {
    /// Creates provenance for one expression and its helper call chain.
    #[must_use]
    pub fn new(
        template_path: impl Into<String>,
        span: SourceSpan,
        helper_chain: Vec<String>,
    ) -> Self {
        Self {
            template_path: template_path.into(),
            span,
            helper_chain,
        }
    }
}
