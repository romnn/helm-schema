use crate::fetch_policy::FetchPolicy;
use crate::load_budget::LoadBudget;

/// Output-only schema transforms selected by CLI flags.
///
/// These transforms run after inference and override merging. They must not
/// feed information back into template analysis.
#[derive(Debug, Clone, Copy)]
pub struct OutputPipelineOptions {
    /// Reference-preservation or inlining policy.
    pub reference_mode: ReferenceMode,
    /// Whether descriptions are removed from final output.
    pub strip_descriptions: bool,
    /// Whether redundant schema structure is minimized.
    pub minimize: bool,
}

/// Input-loading policy for schema documents that must be prepared before
/// final output transforms run.
#[derive(Debug, Clone, Copy)]
pub struct PolicyInputOptions {
    /// Reference policy that determines which documents must be loaded.
    pub reference_mode: ReferenceMode,
    /// Whether required remote documents may be fetched.
    pub fetch_policy: FetchPolicy,
    /// Byte and entry limits applied while loading documents.
    pub load_budget: LoadBudget,
}

/// How final output should handle JSON Schema references.
///
/// This is an output concern only. It controls whether file/URL references are
/// resolved into a self-contained schema or preserved literally for consumers
/// that want to manage references themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceMode {
    /// Bundle referenced schemas while retaining reusable local definitions.
    SelfContained,
    /// Resolve and inline every reachable reference for export.
    FullyInlinedExport,
    /// Preserve references exactly for the downstream consumer.
    PreserveRefs,
}

impl ReferenceMode {
    /// Resolves mutually exclusive CLI flags into one reference policy.
    #[must_use]
    pub fn from_flags(keep_refs: bool, inline_refs: bool) -> Self {
        if keep_refs {
            Self::PreserveRefs
        } else if inline_refs {
            Self::FullyInlinedExport
        } else {
            Self::SelfContained
        }
    }
}

/// JSON serialization format for the final schema document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonOutputFormat {
    /// Indented, human-readable JSON.
    Pretty,
    /// Whitespace-minimized JSON.
    Compact,
}

impl JsonOutputFormat {
    /// Selects compact or pretty JSON from the CLI flag.
    #[must_use]
    pub fn from_compact(compact: bool) -> Self {
        if compact { Self::Compact } else { Self::Pretty }
    }
}

#[cfg(test)]
#[path = "tests/options.rs"]
mod tests;
