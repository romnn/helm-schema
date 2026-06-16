use helm_schema_ir::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::local_override::{
    LocalSchemaLeaf, descend_schema_path_expanding_leaf_with_root_metadata_source,
    expand_local_refs,
};
use crate::lookup::{
    K8sSchemaProvider, ProviderLookupResult, ProviderOrigin, ProviderSchemaFragment,
    ProviderSchemaSource,
};
use crate::metadata_enrichment::enrich_root_metadata_schema;
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
        let source_schema = leaf.source_schema().cloned();
        let mut fragment = ProviderSchemaFragment::new(leaf.into_schema());
        if let Some(source) = source {
            fragment = fragment.with_optional_source_schema(source, source_schema);
        }
        fragment
    }

    #[must_use]
    pub fn materialize_schema_for_resource(&self, resource: &ResourceRef) -> Option<Value> {
        let root = self.universe.schema_doc_for_resource(resource)?;
        let mut stack = std::collections::HashSet::new();
        Some(enrich_root_metadata_schema(expand_local_refs(
            root.root(),
            root.root(),
            0,
            &mut stack,
        )))
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
