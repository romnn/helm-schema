use serde::{Deserialize, Serialize};

/// Byte span within a template source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Source provenance for one emitted contract fact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractProvenance {
    pub template_path: String,
    pub span: SourceSpan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub helper_chain: Vec<String>,
}

impl ContractProvenance {
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
