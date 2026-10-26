use helm_schema_k8s::{
    LocalResourceSchema, LocalSchemaUniverse, resource_schemas_from_crd_document_with_source,
};
use helm_schema_syntax::{
    HelmYamlDocument, HelmYamlKey, MappingEntry, Node, OpaqueKind, Span, TemplatedDocument,
    helm_yaml_documents,
};
use serde_json::{Value, json};

use crate::chart::{ChartContext, FileRole, LoadedChartCorpus};
use crate::error::EngineResult;

const TEMPLATE_CRD_SOURCE_ID: &str = "chart-template-crd";
const STATIC_CRD_SOURCE_ID: &str = "chart-static-crd";

/// Collect chart-local resource schemas from static CRD documents under
/// each chart's `crds/` directory.
#[tracing::instrument(skip_all)]
pub(crate) fn collect_static_crd_universe(
    charts: &[ChartContext],
    corpus: &LoadedChartCorpus,
) -> EngineResult<LocalSchemaUniverse> {
    let mut universe = LocalSchemaUniverse::default();

    for chart in charts {
        for file in corpus.chart(chart)?.files_with_role(FileRole::StaticCrd) {
            for resource_schema in resource_schemas_from_literal_documents(
                file.source()?,
                STATIC_CRD_SOURCE_ID,
                file.path.as_str(),
            ) {
                universe.insert_resource_schema(resource_schema);
            }
        }
    }

    Ok(universe)
}

pub(crate) fn local_resource_schemas_from_template_source(
    source: &str,
    filename: &str,
    contains_template_action: bool,
) -> Vec<LocalResourceSchema> {
    if !contains_template_action {
        return resource_schemas_from_literal_documents(source, TEMPLATE_CRD_SOURCE_ID, filename);
    }

    let document = TemplatedDocument::parse(source);
    let unresolved_keys = unresolved_key_spans(document.roots());
    let mut resource_schemas = Vec::new();
    for span in document.document_spans() {
        if unresolved_keys
            .iter()
            .any(|key| key.start >= span.start && key.start < span.end)
        {
            continue;
        }
        let Some(crd) = crd_document_from_nodes(&document, document.roots(), Some(*span)) else {
            continue;
        };
        resource_schemas.extend(resource_schemas_from_crd_document_with_source(
            &crd,
            TEMPLATE_CRD_SOURCE_ID,
            filename.to_string(),
        ));
    }
    resource_schemas
}

/// An action-led inline key can be attached below its preceding mapping in
/// the CST, so its possible collision cannot be confined to that subtree.
fn unresolved_key_spans(nodes: &[Node]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut pending = nodes.iter().collect::<Vec<_>>();
    while let Some(node) = pending.pop() {
        match node {
            Node::Mapping(entry) => pending.extend(&entry.children),
            Node::Sequence(item) => pending.extend(&item.children),
            Node::Control(region) => {
                for branch in &region.branches {
                    pending.extend(&branch.body);
                }
            }
            Node::Opaque(opaque) => match opaque.kind {
                OpaqueKind::ActionLineText => spans.push(opaque.span),
                OpaqueKind::TemplateComment
                | OpaqueKind::Assignment
                | OpaqueKind::Break
                | OpaqueKind::Continue
                | OpaqueKind::InlineRegion
                | OpaqueKind::ParseError => {}
            },
            Node::Output(_) | Node::Comment(_) | Node::Scalar(_) => {}
        }
    }
    spans
}

/// The CRDs of a literal source, decoded as Helm decodes them.
/// Rejected and unsupported documents contribute no schemas.
/// A version schema containing an unknown value stays open.
fn resource_schemas_from_literal_documents(
    source: &str,
    source_id: &str,
    filename: &str,
) -> Vec<LocalResourceSchema> {
    let mut resource_schemas = Vec::new();
    for document in helm_yaml_documents(source) {
        let HelmYamlDocument::Decoded { mut value, unknown } = document else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        for pointer in &unknown {
            if let Some(schema) =
                enclosing_crd_schema(pointer).and_then(|schema| value.pointer_mut(&schema))
            {
                *schema = json!({});
            }
        }
        resource_schemas.extend(resource_schemas_from_crd_document_with_source(
            &value,
            source_id,
            filename.to_string(),
        ));
    }
    resource_schemas
}

/// The pointer of the CRD version schema enclosing `pointer`, if one does.
fn enclosing_crd_schema(pointer: &str) -> Option<String> {
    let segments = pointer.split('/').collect::<Vec<_>>();
    let length = match segments.as_slice() {
        ["", "spec", "versions", _, "schema", "openAPIV3Schema", ..] => 6,
        ["", "spec", "validation", "openAPIV3Schema", ..] => 4,
        _ => return None,
    };
    Some(segments.get(..length)?.join("/"))
}

fn crd_document_from_nodes(
    document: &TemplatedDocument<'_>,
    nodes: &[Node],
    span: Option<Span>,
) -> Option<Value> {
    let spec = mapping_entry(document, nodes, span, "spec")?;
    if spec.children.is_empty() {
        return Some(json!({
            "apiVersion": literal_string_for_nodes(document, nodes, span, "apiVersion")?,
            "kind": literal_string_for_nodes(document, nodes, span, "kind")?,
            "spec": document.literal_mapping_value(spec)?,
        }));
    }
    let mut spec_json = json!({
        "group": literal_string_for_child(document, spec, "group")?,
        "names": {
            "kind": literal_nested_string_for_child(document, spec, "names", "kind")?,
        },
    });

    if let Some(versions) = mapping_child(document, spec, "versions") {
        let items = versions.sequence_items();
        let versions = if items.is_empty() {
            document
                .literal_mapping_value(versions)?
                .as_array()?
                .iter()
                .map(crd_version_from_value)
                .collect::<Option<Vec<_>>>()?
        } else {
            items
                .into_iter()
                .map(|version| crd_version_from_item(document, version))
                .collect::<Option<Vec<_>>>()?
        };
        spec_json
            .as_object_mut()?
            .insert("versions".to_string(), Value::Array(versions));
    } else {
        let validation = mapping_child(document, spec, "validation")?;
        let spec_object = spec_json.as_object_mut()?;
        spec_object.insert(
            "version".to_string(),
            Value::String(literal_string_for_child(document, spec, "version")?),
        );
        spec_object.insert(
            "validation".to_string(),
            json!({
                "openAPIV3Schema": literal_value_for_child(
                    document,
                    validation,
                    "openAPIV3Schema",
                )?,
            }),
        );
    }

    Some(json!({
        "apiVersion": literal_string_for_nodes(document, nodes, span, "apiVersion")?,
        "kind": literal_string_for_nodes(document, nodes, span, "kind")?,
        "spec": spec_json,
    }))
}

fn crd_version_from_item(
    document: &TemplatedDocument<'_>,
    version: &helm_schema_syntax::SequenceItem,
) -> Option<Value> {
    if !version.children.is_empty() {
        let schema = mapping_entry(document, &version.children, None, "schema")?;
        return Some(json!({
            "name": literal_string_for_nodes(document, &version.children, None, "name")?,
            "served": literal_bool_for_nodes(document, &version.children, None, "served"),
            "schema": {
                "openAPIV3Schema": literal_value_for_child(
                    document,
                    schema,
                    "openAPIV3Schema",
                )?,
            },
        }));
    }

    let version = document.literal_sequence_value(version)?;
    crd_version_from_value(&version)
}

fn crd_version_from_value(version: &Value) -> Option<Value> {
    let version = version.as_object()?;
    let schema = version.get("schema")?.as_object()?;
    Some(json!({
        "name": version.get("name")?.as_str()?,
        "served": version.get("served").and_then(Value::as_bool),
        "schema": { "openAPIV3Schema": schema.get("openAPIV3Schema")? },
    }))
}

fn mapping_child<'nodes>(
    document: &TemplatedDocument<'_>,
    entry: &'nodes MappingEntry,
    key: &str,
) -> Option<&'nodes MappingEntry> {
    mapping_entry(document, &entry.children, None, key)
}

/// The entry for `key` among `nodes`.
/// A merge, duplicate or unresolved key can replace the requested value,
/// so the enclosing mapping stays unknown.
fn mapping_entry<'nodes>(
    document: &TemplatedDocument<'_>,
    nodes: &'nodes [Node],
    span: Option<Span>,
    key: &str,
) -> Option<&'nodes MappingEntry> {
    let mut found = None;
    for node in nodes {
        let Node::Mapping(entry) = node else {
            continue;
        };
        if span.is_some_and(|span| entry.span.start < span.start || entry.span.start >= span.end) {
            continue;
        }
        match document.literal_key(&entry.key) {
            Some(HelmYamlKey::Merge | HelmYamlKey::Unknown) | None => return None,
            Some(HelmYamlKey::Key(name)) if name == key => {
                if found.replace(entry).is_some() {
                    return None;
                }
            }
            Some(HelmYamlKey::Key(_)) => {}
        }
    }
    found
}

fn literal_value_for_child(
    document: &TemplatedDocument<'_>,
    entry: &MappingEntry,
    key: &str,
) -> Option<Value> {
    let entry = mapping_child(document, entry, key)?;
    document.literal_mapping_value(entry)
}

fn literal_string_for_child(
    document: &TemplatedDocument<'_>,
    entry: &MappingEntry,
    key: &str,
) -> Option<String> {
    literal_value_for_child(document, entry, key)?
        .as_str()
        .map(str::to_string)
}

fn literal_nested_string_for_child(
    document: &TemplatedDocument<'_>,
    entry: &MappingEntry,
    key: &str,
    nested_key: &str,
) -> Option<String> {
    let value = literal_value_for_child(document, entry, key)?;
    value
        .as_object()?
        .get(nested_key)?
        .as_str()
        .map(str::to_string)
}

fn literal_string_for_nodes(
    document: &TemplatedDocument<'_>,
    nodes: &[Node],
    span: Option<Span>,
    key: &str,
) -> Option<String> {
    document
        .literal_mapping_value(mapping_entry(document, nodes, span, key)?)?
        .as_str()
        .map(str::to_string)
}

fn literal_bool_for_nodes(
    document: &TemplatedDocument<'_>,
    nodes: &[Node],
    span: Option<Span>,
    key: &str,
) -> Option<bool> {
    document
        .literal_mapping_value(mapping_entry(document, nodes, span, key)?)?
        .as_bool()
}

#[cfg(test)]
#[path = "tests/local_crd_projection.rs"]
mod tests;
