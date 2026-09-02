use helm_schema_k8s::{
    LocalResourceSchema, LocalSchemaUniverse, resource_schemas_from_crd_document_with_source,
};
use helm_schema_syntax::{MappingEntry, Node, Span, TemplatedDocument};
use serde::Deserialize;
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
            )? {
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
) -> EngineResult<Vec<LocalResourceSchema>> {
    if !contains_template_action {
        return resource_schemas_from_literal_documents(source, TEMPLATE_CRD_SOURCE_ID, filename);
    }

    let document = TemplatedDocument::parse(source);
    let mut resource_schemas = Vec::new();
    for span in document.document_spans() {
        let Some(crd) = crd_document_from_nodes(&document, document.roots(), Some(*span)) else {
            continue;
        };
        resource_schemas.extend(resource_schemas_from_crd_document_with_source(
            &crd,
            TEMPLATE_CRD_SOURCE_ID,
            filename.to_string(),
        ));
    }
    Ok(resource_schemas)
}

fn resource_schemas_from_literal_documents(
    source: &str,
    source_id: &str,
    filename: &str,
) -> EngineResult<Vec<LocalResourceSchema>> {
    let mut resource_schemas = Vec::new();
    for document in serde_yaml::Deserializer::from_str(source) {
        let document = Value::deserialize(document)?;
        if document.is_null() {
            continue;
        }
        resource_schemas.extend(resource_schemas_from_crd_document_with_source(
            &document,
            source_id,
            filename.to_string(),
        ));
    }
    Ok(resource_schemas)
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

fn mapping_entry<'nodes>(
    document: &TemplatedDocument<'_>,
    nodes: &'nodes [Node],
    span: Option<Span>,
    key: &str,
) -> Option<&'nodes MappingEntry> {
    nodes.iter().find_map(|node| {
        let Node::Mapping(entry) = node else {
            return None;
        };
        if span.is_some_and(|span| entry.span.start < span.start || entry.span.start >= span.end) {
            return None;
        }
        (document.literal_scalar(&entry.key)?.as_str()? == key).then_some(entry)
    })
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
