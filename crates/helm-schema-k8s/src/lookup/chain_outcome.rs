use super::provider_schema_fragment::ProviderSchemaFragment;

/// Outcome of resolving a known `(apiVersion, kind)` against the full
/// provider chain. Missing diagnostics are projected from the corresponding
/// lookup trace after the chain decides that the miss is final.
// Transient by-value outcome; the size gap between `Resolved` and
// `MissingSchema` does not justify boxing.
#[expect(
    clippy::large_enum_variant,
    reason = "this transient outcome avoids a heap allocation on every successful lookup"
)]
#[derive(Debug)]
pub enum ChainLookupOutcome {
    /// `None` when the resolving provider returned `PathUnresolved`
    /// (intentional silent path-coverage gap).
    Resolved(Option<ProviderSchemaFragment>),
    /// No provider in the chain owns the resource (every provider
    /// returned `NotOwned` or `ResourceDocMissing`).
    MissingSchema,
}

impl ChainLookupOutcome {
    /// Return the resolved schema, intentionally discarding chain metadata.
    pub(crate) fn into_schema_fragment(self) -> Option<ProviderSchemaFragment> {
        match self {
            Self::Resolved(schema) => schema,
            Self::MissingSchema => None,
        }
    }
}
