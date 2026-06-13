use helm_schema_ir::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::local_override::{descend_schema_path_expanding_leaf, expand_local_refs};
use crate::lookup::{K8sSchemaProvider, ProviderLookupResult, ProviderOrigin};
use crate::metadata_enrichment::enrich_root_metadata_schema;
use crate::schema_doc::SchemaDoc;

use super::LocalSchemaUniverse;

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

    fn schema_for_resource_path_from_doc(
        &self,
        root: &SchemaDoc,
        path: &YamlPath,
    ) -> Option<Value> {
        let root = enrich_root_metadata_schema(root.root().clone());
        descend_schema_path_expanding_leaf(&root, &path.0)
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
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        let root = self.universe.schema_doc_for_resource(resource)?;
        self.schema_for_resource_path_from_doc(root, path)
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::ChartLocalCrd
    }

    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        let Some(root) = self.universe.schema_doc_for_resource(resource) else {
            return ProviderLookupResult::NotOwned;
        };

        match self.schema_for_resource_path_from_doc(root, path) {
            Some(schema) => ProviderLookupResult::Found {
                schema,
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

        let schema = provider.schema_for_resource_path(
            &resource("example.com/v1"),
            &YamlPath(vec!["spec".to_string(), "size".to_string()]),
        );

        assert_eq!(schema, Some(json!({"type": "integer"})));
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
