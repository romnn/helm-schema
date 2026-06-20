use std::collections::BTreeMap;
use std::sync::Arc;

use helm_schema_core::ResourceRef;
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
    pub source_id: String,
    pub filename: String,
}

impl LocalResourceSchema {
    #[must_use]
    pub fn new(api_version: impl Into<String>, kind: impl Into<String>, schema: Value) -> Self {
        let api_version = api_version.into();
        let kind = kind.into();
        let filename = stable_resource_schema_filename(&api_version, &kind);
        Self {
            api_version,
            kind,
            schema,
            source_id: "chart-local".to_string(),
            filename,
        }
    }

    #[must_use]
    pub fn with_source(
        mut self,
        source_id: impl Into<String>,
        filename: impl Into<String>,
    ) -> Self {
        self.source_id = source_id.into();
        self.filename = filename.into();
        self
    }
}

/// Chart-local schemas keyed by Kubernetes resource coordinate.
///
/// The universe is source-agnostic: static `crds/` files populate it today,
/// and later A3 document projection can add fully-literal rendered CRDs
/// without changing provider resolution semantics.
#[derive(Clone, Debug, Default)]
pub struct LocalSchemaUniverse {
    docs: BTreeMap<ResourceDocKey, LocalSchemaDocument>,
}

#[derive(Clone, Debug)]
pub(crate) struct LocalSchemaDocument {
    doc: Arc<SchemaDoc>,
    source_id: String,
    filename: String,
}

impl LocalSchemaDocument {
    pub(crate) fn schema_doc(&self) -> &SchemaDoc {
        Arc::as_ref(&self.doc)
    }

    pub(crate) fn source_id(&self) -> &str {
        &self.source_id
    }

    pub(crate) fn filename(&self) -> &str {
        &self.filename
    }
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
        for resource_schema in resource_schemas_from_crd_document(document) {
            insert_resource_schema(&mut self.docs, resource_schema);
        }
    }

    pub fn insert_crd_document_with_source(
        &mut self,
        document: Value,
        source_id: impl Into<String>,
        filename: impl Into<String>,
    ) {
        for resource_schema in
            resource_schemas_from_crd_document_with_source(document, source_id, filename)
        {
            insert_resource_schema(&mut self.docs, resource_schema);
        }
    }

    pub fn insert_resource_schema(&mut self, resource_schema: LocalResourceSchema) {
        insert_resource_schema(&mut self.docs, resource_schema);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    pub(crate) fn schema_doc_for_resource(&self, resource: &ResourceRef) -> Option<&SchemaDoc> {
        self.schema_document_for_resource(resource)
            .map(LocalSchemaDocument::schema_doc)
    }

    pub(crate) fn schema_document_for_resource(
        &self,
        resource: &ResourceRef,
    ) -> Option<&LocalSchemaDocument> {
        self.docs.get(&ResourceDocKey::from_resource(resource))
    }

    pub(crate) fn resource_keys(&self) -> impl Iterator<Item = &ResourceDocKey> {
        self.docs.keys()
    }
}

#[must_use]
pub fn resource_schemas_from_crd_document(document: Value) -> Vec<LocalResourceSchema> {
    resource_schemas_from_crd_document_with_source(document, "chart-local", String::new())
}

#[must_use]
pub fn resource_schemas_from_crd_document_with_source(
    document: Value,
    source_id: impl Into<String>,
    filename: impl Into<String>,
) -> Vec<LocalResourceSchema> {
    let source_id = source_id.into();
    let filename = filename.into();
    let source_filename = (!filename.is_empty()).then_some(filename.as_str());
    let mut resource_schemas = Vec::new();

    if document.pointer("/apiVersion").and_then(Value::as_str) != Some("apiextensions.k8s.io/v1")
        && document.pointer("/apiVersion").and_then(Value::as_str)
            != Some("apiextensions.k8s.io/v1beta1")
    {
        return resource_schemas;
    }
    if document.pointer("/kind").and_then(Value::as_str) != Some("CustomResourceDefinition") {
        return resource_schemas;
    }

    let Some(group) = document.pointer("/spec/group").and_then(Value::as_str) else {
        return resource_schemas;
    };
    let Some(kind) = document.pointer("/spec/names/kind").and_then(Value::as_str) else {
        return resource_schemas;
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
            resource_schemas.push(resource_schema_for_version(
                group,
                name,
                kind,
                schema,
                &source_id,
                source_filename,
            ));
        }
        return resource_schemas;
    }

    let Some(version) = document.pointer("/spec/version").and_then(Value::as_str) else {
        return resource_schemas;
    };
    let Some(schema) = document
        .pointer("/spec/validation/openAPIV3Schema")
        .cloned()
    else {
        return resource_schemas;
    };
    resource_schemas.push(resource_schema_for_version(
        group,
        version,
        kind,
        schema,
        &source_id,
        source_filename,
    ));
    resource_schemas
}

fn resource_schema_for_version(
    group: &str,
    version: &str,
    kind: &str,
    schema: Value,
    source_id: &str,
    source_filename: Option<&str>,
) -> LocalResourceSchema {
    let api_version = format!("{group}/{version}");
    let filename = source_filename
        .map(str::to_string)
        .unwrap_or_else(|| stable_resource_schema_filename(&api_version, kind));
    LocalResourceSchema::new(api_version, kind, schema).with_source(source_id, filename)
}

fn insert_resource_schema(
    docs: &mut BTreeMap<ResourceDocKey, LocalSchemaDocument>,
    resource_schema: LocalResourceSchema,
) {
    let key = ResourceDocKey {
        api_version: resource_schema.api_version,
        kind: resource_schema.kind,
    };
    docs.entry(key).or_insert_with(|| LocalSchemaDocument {
        doc: Arc::new(SchemaDoc::new(resource_schema.schema)),
        source_id: resource_schema.source_id,
        filename: resource_schema.filename,
    });
}

fn stable_resource_schema_filename(api_version: &str, kind: &str) -> String {
    let api_version = api_version.replace('/', "_");
    format!("{api_version}_{kind}.schema.json")
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use test_util::prelude::sim_assert_eq;

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

        sim_assert_eq!(schema, Some(&json!({"type": "integer"})));
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

        sim_assert_eq!(schema, Some(&json!({"type": "boolean"})));
    }
}
