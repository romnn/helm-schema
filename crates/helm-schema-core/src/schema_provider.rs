use crate::{
    ProviderSchemaFragment, ProviderSchemaUse, ResourceRef, YamlPath,
    ordered_api_versions_for_resource,
};

pub fn schema_fragment_for_use_across_ordered_versions(
    use_: &ProviderSchemaUse,
    mut lookup: impl FnMut(&ResourceRef, &YamlPath) -> Option<ProviderSchemaFragment>,
) -> Option<ProviderSchemaFragment> {
    let resource = &use_.resource;
    for version in ordered_api_versions_for_resource(resource) {
        let candidate = ResourceRef {
            api_version: version.to_string(),
            kind: resource.kind.clone(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };
        if let Some(fragment) = lookup(&candidate, &use_.path) {
            return Some(fragment);
        }
    }
    None
}

/// Provides JSON Schema fragments for Kubernetes resource fields.
pub trait ResourceSchemaOracle: Send + Sync + std::fmt::Debug {
    /// Schema for a specific provider-schema lookup request.
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        schema_fragment_for_use_across_ordered_versions(use_, |resource, path| {
            self.schema_fragment_for_resource_path(resource, path)
        })
    }

    fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment>;
}
