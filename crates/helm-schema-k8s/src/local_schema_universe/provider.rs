use helm_schema_core::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::doc_backed_schema::{
    LocalSchemaLeaf, debug_materialize_local_schema,
    descend_schema_path_expanding_leaf_with_root_metadata_source, fragment_for_source_leaf,
};
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::{
    K8sSchemaProvider, ProviderLookupResult, ProviderOrigin, ProviderSchemaFragment,
    ProviderSchemaSource,
};
use crate::schema_doc::SchemaDoc;

use super::LocalSchemaUniverse;
use super::universe::LocalSchemaDocument;

#[derive(Debug)]
pub struct ChartLocalCrdSchemaProvider {
    universe: LocalSchemaUniverse,
    allow_api_version_guess: bool,
}

impl ChartLocalCrdSchemaProvider {
    #[must_use]
    pub fn new(universe: LocalSchemaUniverse) -> Self {
        Self {
            universe,
            allow_api_version_guess: false,
        }
    }

    #[must_use]
    pub fn with_api_version_guess(mut self, enabled: bool) -> Self {
        self.allow_api_version_guess = enabled;
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.universe.is_empty()
    }

    fn schema_leaf_for_resource_path_from_doc(
        &self,
        root: &SchemaDoc,
        path: &YamlPath,
    ) -> Option<LocalSchemaLeaf> {
        descend_schema_path_expanding_leaf_with_root_metadata_source(root.root(), &path.0)
    }

    fn fragment_for_leaf(
        &self,
        document: &LocalSchemaDocument,
        leaf: LocalSchemaLeaf,
    ) -> ProviderSchemaFragment {
        let source = leaf.pointer().map(|pointer| {
            ProviderSchemaSource::new(
                ProviderOrigin::ChartLocalCrd,
                document.source_id().to_string(),
                None,
                document.filename().to_string(),
                pointer.to_string(),
            )
        });
        fragment_for_source_leaf(document.schema_doc(), source, leaf)
    }
}

impl K8sSchemaProvider for ChartLocalCrdSchemaProvider {
    fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        let document = self.universe.schema_document_for_resource(resource)?;
        self.schema_leaf_for_resource_path_from_doc(document.schema_doc(), path)
            .map(|leaf| self.fragment_for_leaf(document, leaf))
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::ChartLocalCrd
    }

    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        let Some(document) = self.universe.schema_document_for_resource(resource) else {
            return ProviderLookupResult::NotOwned;
        };

        match self.schema_leaf_for_resource_path_from_doc(document.schema_doc(), path) {
            Some(leaf) => ProviderLookupResult::Found {
                schema: self.fragment_for_leaf(document, leaf),
                resolved_k8s_version: None,
            },
            None => ProviderLookupResult::PathUnresolved,
        }
    }

    fn has_resource(&self, resource: &ResourceRef) -> bool {
        self.universe.schema_doc_for_resource(resource).is_some()
    }

    fn infer_api_version_candidates(&self, kind: &str) -> Vec<ApiVersionCandidate> {
        if !self.allow_api_version_guess {
            return Vec::new();
        }

        self.universe
            .resource_keys()
            .filter(|key| key.kind() == kind)
            .map(|key| ApiVersionCandidate {
                api_version: key.api_version().to_string(),
                source: InferenceSource::ChartLocalCrd,
                origin: ProviderOrigin::ChartLocalCrd,
            })
            .collect()
    }
}

impl helm_schema_core::ResourceSchemaOracle for ChartLocalCrdSchemaProvider {
    fn schema_fragment_for_use(
        &self,
        use_: &helm_schema_core::ProviderSchemaUse,
    ) -> Option<helm_schema_core::ProviderSchemaFragment> {
        <Self as K8sSchemaProvider>::schema_fragment_for_use(self, use_)
    }

    fn schema_fragment_for_resource_path(
        &self,
        resource: &helm_schema_core::ResourceRef,
        path: &helm_schema_core::YamlPath,
    ) -> Option<helm_schema_core::ProviderSchemaFragment> {
        <Self as K8sSchemaProvider>::schema_fragment_for_resource_path(self, resource, path)
    }

    fn origin(&self) -> helm_schema_core::ProviderOrigin {
        <Self as K8sSchemaProvider>::origin(self)
    }

    fn has_resource(&self, resource: &helm_schema_core::ResourceRef) -> bool {
        <Self as K8sSchemaProvider>::has_resource(self, resource)
    }
}

/// Expand the full chart-local CRD document for regression tests and debugging.
///
/// Production provider lookup stays on the fragment-first path.
#[must_use]
pub fn debug_materialize_schema_for_resource(
    provider: &ChartLocalCrdSchemaProvider,
    resource: &ResourceRef,
) -> Option<Value> {
    let root = provider.universe.schema_doc_for_resource(resource)?;
    Some(debug_materialize_local_schema(root.root()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn resource(api_version: &str) -> ResourceRef {
        ResourceRef {
            api_version: api_version.to_string(),
            kind: "Widget".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }
    }

    fn widget_universe() -> LocalSchemaUniverse {
        LocalSchemaUniverse::from_crd_documents([json!({
            "apiVersion": "apiextensions.k8s.io/v1",
            "kind": "CustomResourceDefinition",
            "spec": {
                "group": "example.com",
                "names": {"kind": "Widget"},
                "versions": [
                    {
                        "name": "v1",
                        "served": true,
                        "schema": {
                            "openAPIV3Schema": {
                                "type": "object",
                                "properties": {
                                    "spec": {
                                        "type": "object",
                                        "properties": {
                                            "size": {"type": "integer"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                ]
            }
        })])
    }

    #[test]
    fn resolves_served_crd_version_schema_from_universe() {
        let provider = ChartLocalCrdSchemaProvider::new(widget_universe());

        let schema = provider.schema_fragment_for_resource_path(
            &resource("example.com/v1"),
            &YamlPath(vec!["spec".to_string(), "size".to_string()]),
        );

        assert_eq!(
            schema.map(ProviderSchemaFragment::into_schema),
            Some(json!({"type": "integer"}))
        );
    }

    #[test]
    fn lookup_attaches_chart_local_provider_source() {
        let mut universe = LocalSchemaUniverse::default();
        universe.insert_crd_document_with_source(
            json!({
                "apiVersion": "apiextensions.k8s.io/v1",
                "kind": "CustomResourceDefinition",
                "spec": {
                    "group": "example.com",
                    "names": {"kind": "Widget"},
                    "versions": [
                        {
                            "name": "v1",
                            "served": true,
                            "schema": {
                                "openAPIV3Schema": {
                                    "type": "object",
                                    "properties": {
                                        "spec": {
                                            "type": "object",
                                            "properties": {
                                                "size": {"type": "integer"}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    ]
                }
            }),
            "chart-static-crd",
            "/chart/crds/widgets.yaml",
        );
        let provider = ChartLocalCrdSchemaProvider::new(universe);

        let result = provider.lookup(
            &resource("example.com/v1"),
            &YamlPath(vec!["spec".to_string(), "size".to_string()]),
        );
        let ProviderLookupResult::Found { schema, .. } = result else {
            panic!("chart-local lookup should resolve spec.size");
        };
        let source = schema.source().expect("chart-local source should attach");

        assert_eq!(source.origin(), ProviderOrigin::ChartLocalCrd);
        assert_eq!(source.source_id(), "chart-static-crd");
        assert_eq!(source.filename(), "/chart/crds/widgets.yaml");
        assert_eq!(source.pointer(), "/properties/spec/properties/size");
    }

    #[test]
    fn api_version_guess_uses_chart_local_crd_origin() {
        let provider =
            ChartLocalCrdSchemaProvider::new(widget_universe()).with_api_version_guess(true);

        assert_eq!(
            provider.infer_api_version_candidates("Widget"),
            vec![ApiVersionCandidate {
                api_version: "example.com/v1".to_string(),
                source: InferenceSource::ChartLocalCrd,
                origin: ProviderOrigin::ChartLocalCrd,
            }]
        );
    }
}
