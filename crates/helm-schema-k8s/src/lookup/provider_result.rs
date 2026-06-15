use super::provider_schema_fragment::ProviderSchemaFragment;

/// Result of a single provider answering "do you own this resource,
/// and if so, can you resolve this path?". Provider-local: emits no
/// diagnostics directly. The chain ([`crate::lookup::Chain`])
/// records these outcomes in a lookup trace, projects diagnostics
/// from final misses, and returns the public
/// [`crate::lookup::ChainLookupOutcome`].
#[derive(Clone, Debug)]
pub enum ProviderLookupResult {
    /// Provider owns the resource AND resolved the requested path.
    /// `resolved_k8s_version` is `Some(...)` when the K8s provider
    /// answered via a non-primary version (Feature B); the chain uses
    /// it to emit `ResolvedFromFallbackVersion`.
    Found {
        schema: ProviderSchemaFragment,
        resolved_k8s_version: Option<String>,
    },

    /// Provider owns the resource AND found the resource doc, but the
    /// requested YAML path is not present in it. NOT a missing-schema
    /// situation — preserves the intentional "silent coverage gap"
    /// behaviour: a chart referencing `.foo.bar.baz` where the schema
    /// only documents `.foo.bar` produces no warning. The chain treats
    /// this exactly like `Found { schema: null }` for diagnostic
    /// purposes.
    PathUnresolved,

    /// Provider owns the resource (claimed it in `has_resource`) but
    /// its expected source file is genuinely missing — e.g. a transient
    /// fetch error in `schema_for_resource_path` after `has_resource`
    /// returned true. Rare; the chain treats this as equivalent to
    /// `NotOwned` and moves on (since some other provider may still
    /// have it). Local overrides are the exception: see the chain's
    /// origin-specific handling.
    ///
    /// `source_path` is the filesystem path the provider tried to read
    /// (when applicable — non-local providers may leave it empty).
    /// Threaded to `Diagnostic::LocalOverrideUnreadable.override_path`
    /// when the local override layer is the one reporting.
    ResourceDocMissing {
        io_error: String,
        source_path: String,
    },

    /// Provider does not own the resource at any configured version.
    /// Chain moves on to the next provider.
    NotOwned,
}
