use serde_json::Value;

use super::provider_origin::ProviderOrigin;

/// Outcome of resolving a known `(apiVersion, kind)` against the full
/// provider chain. Chain-level; this is the only place
/// [`crate::diagnostic::Diagnostic::MissingSchema`] is emitted from.
#[derive(Debug)]
pub enum ChainLookupOutcome {
    Resolved {
        /// `None` when the resolving provider returned `PathUnresolved`
        /// (intentional silent path-coverage gap).
        schema: Option<Value>,
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
