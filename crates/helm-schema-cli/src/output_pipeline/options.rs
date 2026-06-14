/// Output-only schema transforms selected by CLI flags.
///
/// These transforms run after inference and override merging. They must not
/// feed information back into template analysis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputPipelineOptions {
    pub(crate) reference_handling: ReferenceHandling,
    pub(crate) allow_net: bool,
    pub(crate) strip_descriptions: bool,
    pub(crate) minimize: bool,
}

/// How final output should handle JSON Schema `$ref` nodes.
///
/// This is an output concern only. It controls whether the final schema stays
/// as a bundled document with references or is exported as a fully flattened
/// document for consumers that cannot resolve references themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceHandling {
    PreserveRefs,
    FlattenedExport,
}

impl ReferenceHandling {
    pub(crate) fn from_keep_refs(keep_refs: bool) -> Self {
        if keep_refs {
            Self::PreserveRefs
        } else {
            Self::FlattenedExport
        }
    }

    pub(super) fn flatten(self) -> bool {
        matches!(self, Self::FlattenedExport)
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
