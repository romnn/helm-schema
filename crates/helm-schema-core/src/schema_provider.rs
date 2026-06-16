use crate::{
    ProviderOrigin, ProviderSchemaFragment, ProviderSchemaUse, ResourceRef, YamlPath,
    ordered_api_versions_for_resource,
};

/// Provides JSON Schema fragments for Kubernetes resource fields.
pub trait ResourceSchemaOracle: Send + Sync + std::fmt::Debug {
    /// Schema for a specific provider-schema lookup request.
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        let resource = &use_.resource;
        for version in ordered_api_versions_for_resource(resource) {
            let candidate = ResourceRef {
                api_version: version.to_string(),
                kind: resource.kind.clone(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            };
            if let Some(fragment) = self.schema_fragment_for_resource_path(&candidate, &use_.path) {
                return Some(fragment);
            }
        }
        None
    }

    fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment>;

    fn origin(&self) -> ProviderOrigin;

    fn has_resource(&self, resource: &ResourceRef) -> bool;
}
