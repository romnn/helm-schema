use std::collections::BTreeMap;
use std::sync::Arc;

use helm_schema_ir::ResourceRef;
use serde_json::Value;

use crate::schema_doc::SchemaDoc;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ResourceDocKey {
    api_version: String,
    kind: String,
}

impl ResourceDocKey {
    pub(crate) fn from_resource(resource: &ResourceRef) -> Self {
        Self {
            api_version: resource.api_version.clone(),
            kind: resource.kind.clone(),
        }
    }

    pub(crate) fn api_version(&self) -> &str {
        &self.api_version
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }
}

/// A schema document for one concrete Kubernetes resource coordinate.
///
/// Static CRDs are one producer of this type today. Later, fully-literal
/// rendered document projection can produce the same type without adding a
/// second chart-local provider path.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalResourceSchema {
    pub api_version: String,
    pub kind: String,
    pub schema: Value,
}

impl LocalResourceSchema {
    #[must_use]
    pub fn new(api_version: impl Into<String>, kind: impl Into<String>, schema: Value) -> Self {
        Self {
            api_version: api_version.into(),
            kind: kind.into(),
            schema,
        }
    }
}

/// Chart-local schemas keyed by Kubernetes resource coordinate.
///
/// The universe is source-agnostic: static `crds/` files populate it today,
/// and later A3 document projection can add fully-literal rendered CRDs
/// without changing provider resolution semantics.
#[derive(Clone, Debug, Default)]
pub struct LocalSchemaUniverse {
    docs: BTreeMap<ResourceDocKey, Arc<SchemaDoc>>,
}

impl LocalSchemaUniverse {
    #[must_use]
    pub fn from_crd_documents<I>(documents: I) -> Self
    where
        I: IntoIterator<Item = Value>,
    {
        let mut universe = Self::default();
        for document in documents {
            universe.insert_crd_document(document);
        }
        universe
    }

    pub fn insert_crd_document(&mut self, document: Value) {
        insert_crd_versions(&mut self.docs, document);
    }

    pub fn insert_resource_schema(&mut self, resource_schema: LocalResourceSchema) {
        insert_resource_schema(&mut self.docs, resource_schema);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    pub(crate) fn schema_doc_for_resource(&self, resource: &ResourceRef) -> Option<&SchemaDoc> {
        self.docs
            .get(&ResourceDocKey::from_resource(resource))
            .map(Arc::as_ref)
    }

    pub(crate) fn resource_keys(&self) -> impl Iterator<Item = &ResourceDocKey> {
        self.docs.keys()
    }
}

fn insert_crd_versions(docs: &mut BTreeMap<ResourceDocKey, Arc<SchemaDoc>>, document: Value) {
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
    docs: &mut BTreeMap<ResourceDocKey, Arc<SchemaDoc>>,
    group: &str,
    version: &str,
    kind: &str,
    schema: Value,
) {
    insert_resource_schema(
        docs,
        LocalResourceSchema::new(format!("{group}/{version}"), kind, schema),
    );
}

fn insert_resource_schema(
    docs: &mut BTreeMap<ResourceDocKey, Arc<SchemaDoc>>,
    resource_schema: LocalResourceSchema,
) {
    let key = ResourceDocKey {
        api_version: resource_schema.api_version,
        kind: resource_schema.kind,
    };
    docs.entry(key)
        .or_insert_with(|| Arc::new(SchemaDoc::new(resource_schema.schema)));
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
        let universe = LocalSchemaUniverse::from_crd_documents([json!({
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
        })]);

        let schema = universe
            .schema_doc_for_resource(&resource("example.com/v1"))
            .and_then(|schema_doc| {
                schema_doc
                    .root()
                    .pointer("/properties/spec/properties/size")
            });

        assert_eq!(schema, Some(&json!({"type": "integer"})));
    }

    #[test]
    fn ignores_unserved_crd_versions() {
        let universe = LocalSchemaUniverse::from_crd_documents([json!({
            "apiVersion": "apiextensions.k8s.io/v1",
            "kind": "CustomResourceDefinition",
            "spec": {
                "group": "example.com",
                "names": {"kind": "Widget"},
                "versions": [
                    {
                        "name": "v1",
                        "served": false,
                        "schema": {"openAPIV3Schema": {"type": "object"}}
                    }
                ]
            }
        })]);

        assert!(
            universe
                .schema_doc_for_resource(&resource("example.com/v1"))
                .is_none()
        );
    }

    #[test]
    fn inserts_direct_resource_schema_without_crd_envelope() {
        let mut universe = LocalSchemaUniverse::default();
        universe.insert_resource_schema(LocalResourceSchema::new(
            "example.com/v1",
            "Widget",
            json!({
                "type": "object",
                "properties": {
                    "spec": {
                        "type": "object",
                        "properties": {
                            "enabled": {"type": "boolean"}
                        }
                    }
                }
            }),
        ));

        let schema = universe
            .schema_doc_for_resource(&resource("example.com/v1"))
            .and_then(|schema_doc| {
                schema_doc
                    .root()
                    .pointer("/properties/spec/properties/enabled")
            });

        assert_eq!(schema, Some(&json!({"type": "boolean"})));
    }
}
