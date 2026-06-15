/// Output-only schema transforms selected by CLI flags.
///
/// These transforms run after inference and override merging. They must not
/// feed information back into template analysis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputPipelineOptions {
    pub(crate) reference_mode: ReferenceMode,
    pub(crate) strip_descriptions: bool,
    pub(crate) minimize: bool,
}

/// Input-loading policy for schema documents that must be prepared before
/// final output transforms run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PolicyInputOptions {
    pub(crate) reference_mode: ReferenceMode,
    pub(crate) allow_net: bool,
}

/// How final output should handle JSON Schema references.
///
/// This is an output concern only. It controls whether file/URL references are
/// resolved into a self-contained schema or preserved literally for consumers
/// that want to manage references themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceMode {
    SelfContained,
    FullyInlinedExport,
    PreserveRefs,
}

impl ReferenceMode {
    pub(crate) fn from_flags(keep_refs: bool, inline_refs: bool) -> Self {
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
            ReferenceMode::from_flags(false, false),
            ReferenceMode::SelfContained
        );
        assert!(ReferenceMode::SelfContained.bundles_refs());
        assert!(!ReferenceMode::SelfContained.fully_inlines_refs());
    }

    #[test]
    fn keep_refs_selects_reference_preserving_output() {
        assert_eq!(
            ReferenceMode::from_flags(true, false),
            ReferenceMode::PreserveRefs
        );
        assert!(!ReferenceMode::PreserveRefs.bundles_refs());
        assert!(!ReferenceMode::PreserveRefs.fully_inlines_refs());
    }

    #[test]
    fn inline_refs_selects_fully_inlined_export_output() {
        assert_eq!(
            ReferenceMode::from_flags(false, true),
            ReferenceMode::FullyInlinedExport
        );
        assert!(!ReferenceMode::FullyInlinedExport.bundles_refs());
        assert!(ReferenceMode::FullyInlinedExport.fully_inlines_refs());
    }
}
