use std::collections::BTreeMap;

use helm_schema_ir::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::local_override::{descend_schema_path_expanding_leaf, expand_local_refs};
use crate::lookup::{K8sSchemaProvider, ProviderLookupResult, ProviderOrigin};
use crate::metadata_enrichment::enrich_root_metadata_schema;
use crate::schema_doc::SchemaDoc;

#[derive(Clone, Debug)]
pub struct ChartLocalCrdSource {
    pub document: Value,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ResourceDocKey {
    api_version: String,
    kind: String,
}

impl ResourceDocKey {
    fn from_resource(resource: &ResourceRef) -> Self {
        Self {
            api_version: resource.api_version.clone(),
            kind: resource.kind.clone(),
        }
    }
}

#[derive(Debug)]
pub struct ChartLocalCrdSchemaProvider {
    docs: BTreeMap<ResourceDocKey, SchemaDoc>,
    allow_api_version_guess: bool,
}

impl ChartLocalCrdSchemaProvider {
    #[must_use]
    pub fn new(sources: Vec<ChartLocalCrdSource>) -> Self {
        let mut docs = BTreeMap::new();
        for source in sources {
            insert_crd_versions(&mut docs, source.document);
        }

        Self {
            docs,
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
        self.docs.is_empty()
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
        let root = self.docs.get(&ResourceDocKey::from_resource(resource))?;
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
        let root = self.docs.get(&ResourceDocKey::from_resource(resource))?;
        self.schema_for_resource_path_from_doc(root, path)
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::ChartLocalCrd
    }

    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        let Some(root) = self.docs.get(&ResourceDocKey::from_resource(resource)) else {
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
        self.docs
            .contains_key(&ResourceDocKey::from_resource(resource))
    }

    fn infer_api_version_candidates(&self, kind: &str) -> Vec<ApiVersionCandidate> {
        if !self.allow_api_version_guess {
            return Vec::new();
        }

        self.docs
            .keys()
            .filter(|key| key.kind == kind)
            .map(|key| ApiVersionCandidate {
                api_version: key.api_version.clone(),
                source: InferenceSource::ChartLocalCrd,
                origin: ProviderOrigin::ChartLocalCrd,
            })
            .collect()
    }
}

fn insert_crd_versions(docs: &mut BTreeMap<ResourceDocKey, SchemaDoc>, document: Value) {
    if document.pointer("/apiVersion").and_then(Value::as_str) != Some("apiextensions.k8s.io/v1")
        && document.pointer("/apiVersion").and_then(Value::as_str)
            != Some("apiextensions.k8s.io/v1beta1")
    {
        return;
    }
    if document.pointer("/kind").and_then(Value::as_str) != Some("CustomResourceDefinition") {
        return;
    }

    let Some(group) = document.pointer("/spec/group").and_then(Value::as_str) else {
        return;
    };
    let Some(kind) = document.pointer("/spec/names/kind").and_then(Value::as_str) else {
        return;
    };

    if let Some(versions) = document.pointer("/spec/versions").and_then(Value::as_array) {
        for version in versions {
            if version
                .get("served")
                .and_then(Value::as_bool)
                .is_some_and(|served| !served)
            {
                continue;
            }
            let Some(name) = version.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(schema) = version.pointer("/schema/openAPIV3Schema").cloned() else {
                continue;
            };
            insert_schema_doc(docs, group, name, kind, schema);
        }
        return;
    }

    let Some(version) = document.pointer("/spec/version").and_then(Value::as_str) else {
        return;
    };
    let Some(schema) = document
        .pointer("/spec/validation/openAPIV3Schema")
        .cloned()
    else {
        return;
    };
    insert_schema_doc(docs, group, version, kind, schema);
}

fn insert_schema_doc(
    docs: &mut BTreeMap<ResourceDocKey, SchemaDoc>,
    group: &str,
    version: &str,
    kind: &str,
    schema: Value,
) {
    let key = ResourceDocKey {
        api_version: format!("{group}/{version}"),
        kind: kind.to_string(),
    };
    docs.entry(key).or_insert_with(|| SchemaDoc::new(schema));
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

    #[test]
    fn extracts_served_crd_version_schema() {
        let provider = ChartLocalCrdSchemaProvider::new(vec![ChartLocalCrdSource {
            document: json!({
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
        }]);

        let schema = provider.schema_for_resource_path(
            &resource("example.com/v1"),
            &YamlPath(vec!["spec".to_string(), "size".to_string()]),
        );

        assert_eq!(schema, Some(json!({"type": "integer"})));
    }

    #[test]
    fn api_version_guess_uses_chart_local_crd_origin() {
        let provider = ChartLocalCrdSchemaProvider::new(vec![ChartLocalCrdSource {
            document: json!({
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
                                "openAPIV3Schema": {"type": "object"}
                            }
                        }
                    ]
                }
            }),
        }])
        .with_api_version_guess(true);

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
