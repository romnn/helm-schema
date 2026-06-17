/// Byte span within a template source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractProvenance {
    pub template_path: String,
    pub span: SourceSpan,
}

impl ContractProvenance {
    #[must_use]
    pub fn new(template_path: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            template_path: template_path.into(),
            span,
        }
    }
}
