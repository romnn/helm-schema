use super::provider_origin::ProviderOrigin;
use super::provider_schema_fragment::ProviderSchemaFragment;

/// Outcome of resolving a known `(apiVersion, kind)` against the full
/// provider chain. Missing diagnostics are projected from the corresponding
/// lookup trace after the chain decides that the miss is final.
// Transient by-value outcome; the size gap between `Resolved` and
// `MissingSchema` does not justify boxing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ChainLookupOutcome {
    Resolved {
        /// `None` when the resolving provider returned `PathUnresolved`
        /// (intentional silent path-coverage gap).
        schema: Option<ProviderSchemaFragment>,
        resolving_provider: ProviderOrigin,
        resolved_k8s_version: Option<String>,
    },
    /// No provider in the chain owns the resource (every provider
    /// returned `NotOwned` or `ResourceDocMissing`). Chain layer emits
    /// `Diagnostic::MissingSchema` with the union of K8s versions and
    /// filenames tried.
    MissingSchema {
        k8s_versions_tried: Vec<String>,
        tried_filenames: Vec<String>,
    },
}

impl ChainLookupOutcome {
    /// Return the resolved schema, intentionally discarding chain metadata.
    pub(crate) fn into_schema_fragment(self) -> Option<ProviderSchemaFragment> {
        match self {
            Self::Resolved { schema, .. } => schema,
            Self::MissingSchema { .. } => None,
        }
    }
}
