use std::collections::BTreeMap;
use std::sync::Arc;

use helm_schema_core::ResourceRef;
use serde_json::Value;

use crate::schema_doc::SchemaDoc;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ResourceDocKey {
    pub(crate) api_version: String,
    pub(crate) kind: String,
}

impl ResourceDocKey {
    pub(crate) fn from_resource(resource: &ResourceRef) -> Self {
        Self {
            api_version: resource.api_version.clone(),
            kind: resource.kind.clone(),
        }
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
    pub(crate) doc: Arc<SchemaDoc>,
    pub(crate) source_id: String,
    pub(crate) filename: String,
}

impl LocalSchemaUniverse {
    pub fn insert_resource_schema(&mut self, resource_schema: LocalResourceSchema) {
        let key = ResourceDocKey {
            api_version: resource_schema.api_version,
            kind: resource_schema.kind,
        };
        self.docs.entry(key).or_insert_with(|| LocalSchemaDocument {
            doc: Arc::new(SchemaDoc::new(resource_schema.schema)),
            source_id: resource_schema.source_id,
            filename: resource_schema.filename,
        });
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    pub(crate) fn schema_doc_for_resource(&self, resource: &ResourceRef) -> Option<&SchemaDoc> {
        self.schema_document_for_resource(resource)
            .map(|document| document.doc.as_ref())
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
pub fn resource_schemas_from_crd_document_with_source(
    document: Value,
    source_id: impl Into<String>,
    filename: impl Into<String>,
) -> Vec<LocalResourceSchema> {
    let source_id = source_id.into();
    let filename = filename.into();
    let source_filename = (!filename.is_empty()).then_some(filename.as_str());
    let mut resource_schemas = Vec::new();

    let api_version = document.pointer("/apiVersion").and_then(Value::as_str);
    if !matches!(
        api_version,
        Some("apiextensions.k8s.io/v1" | "apiextensions.k8s.io/v1beta1")
    ) {
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
    let filename = source_filename.map_or_else(
        || stable_resource_schema_filename(&api_version, kind),
        str::to_string,
    );
    LocalResourceSchema {
        api_version,
        kind: kind.to_string(),
        schema,
        source_id: source_id.to_string(),
        filename,
    }
}

fn stable_resource_schema_filename(api_version: &str, kind: &str) -> String {
    let api_version = api_version.replace('/', "_");
    format!("{api_version}_{kind}.schema.json")
}

#[cfg(test)]
#[path = "tests/universe.rs"]
mod tests;
