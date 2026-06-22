use crate::fetch_policy::FetchPolicy;
use crate::load_budget::LoadBudget;

/// Output-only schema transforms selected by CLI flags.
///
/// These transforms run after inference and override merging. They must not
/// feed information back into template analysis.
#[derive(Debug, Clone, Copy)]
pub struct OutputPipelineOptions {
    pub reference_mode: ReferenceMode,
    pub strip_descriptions: bool,
    pub minimize: bool,
}

/// Input-loading policy for schema documents that must be prepared before
/// final output transforms run.
#[derive(Debug, Clone, Copy)]
pub struct PolicyInputOptions {
    pub reference_mode: ReferenceMode,
    pub fetch_policy: FetchPolicy,
    pub load_budget: LoadBudget,
}

/// How final output should handle JSON Schema references.
///
/// This is an output concern only. It controls whether file/URL references are
/// resolved into a self-contained schema or preserved literally for consumers
/// that want to manage references themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceMode {
    SelfContained,
    FullyInlinedExport,
    PreserveRefs,
}

impl ReferenceMode {
    pub fn from_flags(keep_refs: bool, inline_refs: bool) -> Self {
        if keep_refs {
            Self::PreserveRefs
        } else if inline_refs {
            Self::FullyInlinedExport
        } else {
            Self::SelfContained
        }
    }

    pub(super) fn bundles_refs(self) -> bool {
        matches!(self, Self::SelfContained)
    }

    pub(super) fn fully_inlines_refs(self) -> bool {
        matches!(self, Self::FullyInlinedExport)
    }
}

/// JSON serialization format for the final schema document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonOutputFormat {
    Pretty,
    Compact,
}

impl JsonOutputFormat {
    pub fn from_compact(compact: bool) -> Self {
        if compact { Self::Compact } else { Self::Pretty }
    }
}

#[cfg(test)]
#[path = "tests/options.rs"]
mod tests;
