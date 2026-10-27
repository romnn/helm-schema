use crate::{ProviderSchemaFragment, ProviderSchemaUse, ResourceRef};

/// Provides JSON Schema fragments for Kubernetes resource fields.
pub trait ResourceSchemaOracle: Send + Sync + std::fmt::Debug {
    /// Observes a potentially emitted resource for provider diagnostics without a value-slot claim.
    fn observe_resource(&self, _resource: &ResourceRef) {}

    /// Schema for a specific provider-schema lookup request.
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment>;
}
