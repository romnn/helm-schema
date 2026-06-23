use crate::{ProviderSchemaFragment, ProviderSchemaUse};

/// Provides JSON Schema fragments for Kubernetes resource fields.
pub trait ResourceSchemaOracle: Send + Sync + std::fmt::Debug {
    /// Schema for a specific provider-schema lookup request.
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment>;
}
