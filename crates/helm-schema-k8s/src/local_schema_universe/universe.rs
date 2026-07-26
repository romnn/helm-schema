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
    /// Concrete API version declared by the CRD.
    pub api_version: String,
    /// Kubernetes kind declared by the CRD.
    pub kind: String,
    /// `OpenAPI` schema for instances of the resource.
    pub schema: Value,
    /// Stable identity of the chart-local source.
    pub source_id: String,
    /// Logical filename used in provider provenance.
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
    /// Inserts a resource schema unless that coordinate already has a document.
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

    /// Reports whether the universe contains no resource schemas.
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

/// Extracts served resource schemas from a CRD document with source provenance.
#[must_use]
pub fn resource_schemas_from_crd_document_with_source(
    document: &Value,
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

    let prunes = crd_prunes_unknown_fields(document, api_version);

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
            let Some(mut schema) = version.pointer("/schema/openAPIV3Schema").cloned() else {
                continue;
            };
            if prunes {
                close_pruned_object_nodes(&mut schema, true);
            }
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
    let Some(mut schema) = document
        .pointer("/spec/validation/openAPIV3Schema")
        .cloned()
    else {
        return resource_schemas;
    };
    if prunes {
        close_pruned_object_nodes(&mut schema, true);
    }
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

/// Whether the API server prunes fields the CRD's schema does not declare.
///
/// `apiextensions.k8s.io/v1` only accepts structural schemas and always
/// prunes; `v1beta1` keeps unknown fields unless the CRD opts in.
fn crd_prunes_unknown_fields(document: &Value, api_version: Option<&str>) -> bool {
    if api_version == Some("apiextensions.k8s.io/v1") {
        return true;
    }
    document
        .pointer("/spec/preserveUnknownFields")
        .and_then(Value::as_bool)
        == Some(false)
}

/// Stamps a pruning CRD's structural contract onto its object nodes.
///
/// A pruned resource carries only the fields its schema declares, so an
/// object that declares `properties` accepts nothing else. The CRDs catalog
/// bakes that stamp into its documents; without it here, a chart that ships
/// the very same CRD would hand out an open contract for a resource the
/// catalog closes, and which provider answered would decide what the values
/// schema accepts.
///
/// Three subtrees keep the open reading. `metadata` is validated by the API
/// machinery rather than the structural schema and is never pruned; a
/// subtree can opt out with `x-kubernetes-preserve-unknown-fields`; and an
/// embedded resource carries a schema for a foreign object, which charts
/// routinely declare only in part. Only the structural keywords are walked:
/// a structural schema states its structure outside `allOf`/`anyOf`/`oneOf`/
/// `not`, so a stamp inside a junctor arm would reject documents the sibling
/// arm accepts.
fn close_pruned_object_nodes(schema: &mut Value, root: bool) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    for opt_out in [
        "x-kubernetes-preserve-unknown-fields",
        "x-kubernetes-embedded-resource",
    ] {
        if object.get(opt_out).and_then(Value::as_bool) == Some(true) {
            return;
        }
    }

    let mut declares_properties = false;
    if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
        declares_properties = !properties.is_empty();
        for (key, child) in properties.iter_mut() {
            if root && key == "metadata" {
                continue;
            }
            close_pruned_object_nodes(child, false);
        }
    }
    for key in ["items", "additionalProperties"] {
        if let Some(child) = object.get_mut(key) {
            close_pruned_object_nodes(child, false);
        }
    }

    // The document root keeps its open reading, matching the catalog
    // conversion: `apiVersion`, `kind`, and `metadata` reach every custom
    // resource whether or not the CRD spells them.
    if !root && declares_properties && !object.contains_key("additionalProperties") {
        object.insert("additionalProperties".to_string(), Value::Bool(false));
    }
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
