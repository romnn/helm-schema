/// Output-only schema transforms selected by CLI flags.
///
/// These transforms run after inference and override merging. They must not
/// feed information back into template analysis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputPipelineOptions {
    pub(crate) reference_mode: ReferenceMode,
    pub(crate) allow_net: bool,
    pub(crate) strip_descriptions: bool,
    pub(crate) minimize: bool,
}

/// How final output should handle JSON Schema references.
///
/// This is an output concern only. It controls whether file/URL references are
/// resolved into a self-contained schema or preserved literally for consumers
/// that want to manage references themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceMode {
    SelfContained,
    PreserveRefs,
}

impl ReferenceMode {
    pub(crate) fn from_keep_refs(keep_refs: bool) -> Self {
        if keep_refs {
            Self::PreserveRefs
        } else {
            Self::SelfContained
        }
    }

    pub(super) fn dereference(self) -> bool {
        matches!(self, Self::SelfContained)
    }
}

/// JSON serialization format for the final schema document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonOutputFormat {
    Pretty,
    Compact,
}

impl JsonOutputFormat {
    pub(crate) fn from_compact(compact: bool) -> Self {
        if compact { Self::Compact } else { Self::Pretty }
    }
}

#[cfg(test)]
mod tests {
    use super::ReferenceMode;

    #[test]
    fn reference_mode_defaults_to_self_contained_output() {
        assert_eq!(
            ReferenceMode::from_keep_refs(false),
            ReferenceMode::SelfContained
        );
        assert!(ReferenceMode::SelfContained.dereference());
    }

    #[test]
    fn keep_refs_selects_reference_preserving_output() {
        assert_eq!(
            ReferenceMode::from_keep_refs(true),
            ReferenceMode::PreserveRefs
        );
        assert!(!ReferenceMode::PreserveRefs.dereference());
    }
}
