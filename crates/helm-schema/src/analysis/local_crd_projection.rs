use helm_schema_ast::{ParseError, parse_helm_template};
use helm_schema_k8s::{
    LocalResourceSchema, LocalSchemaUniverse, resource_schemas_from_crd_document_with_source,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::chart::{self, ChartContext, FileRole};
use crate::error::EngineResult;

const TEMPLATE_CRD_SOURCE_ID: &str = "chart-template-crd";
const STATIC_CRD_SOURCE_ID: &str = "chart-static-crd";

/// Collect chart-local resource schemas from static CRD documents under
/// each chart's `crds/` directory.
#[tracing::instrument(skip_all)]
pub(crate) fn collect_static_crd_universe(
    charts: &[ChartContext],
) -> EngineResult<LocalSchemaUniverse> {
    let mut universe = LocalSchemaUniverse::default();

    for chart in charts {
        for path in chart::files_with_role(&chart.chart_dir, false, FileRole::StaticCrd)? {
            let source = path.read_to_string()?;
            for resource_schema in resource_schemas_from_literal_documents(
                &source,
                STATIC_CRD_SOURCE_ID,
                path.as_str(),
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

    let tree = parse_helm_template(source).ok_or(ParseError::TreeSitterParseFailed)?;
    let mut resource_schemas = Vec::new();
    collect_template_crd_schemas(tree.root_node(), source, filename, &mut resource_schemas);
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
            document,
            source_id,
            filename.to_string(),
        ));
    }
    Ok(resource_schemas)
}

fn collect_template_crd_schemas(
    node: tree_sitter::Node<'_>,
    source: &str,
    filename: &str,
    resource_schemas: &mut Vec<LocalResourceSchema>,
) {
    if let Some(document) = crd_document_from_node(node, source) {
        let schemas = resource_schemas_from_crd_document_with_source(
            document,
            TEMPLATE_CRD_SOURCE_ID,
            filename.to_string(),
        );
        if !schemas.is_empty() {
            resource_schemas.extend(schemas);
            return;
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_template_crd_schemas(child, source, filename, resource_schemas);
    }
}

fn crd_document_from_node(node: tree_sitter::Node<'_>, source: &str) -> Option<Value> {
    let spec = mapping_value(node, source, "spec")?;
    let names = mapping_value(spec, source, "names")?;
    let mut spec_json = json!({
        "group": literal_string_for_key(spec, source, "group")?,
        "names": { "kind": literal_string_for_key(names, source, "kind")? },
    });

    if let Some(version_nodes) = mapping_value(spec, source, "versions").and_then(sequence_items) {
        let versions = version_nodes
            .iter()
            .map(|version| {
                let schema = mapping_value(*version, source, "schema")
                    .and_then(|schema| mapping_value(schema, source, "openAPIV3Schema"))
                    .and_then(|schema| literal_json_from_node(schema, source))?;
                Some(json!({
                    "name": literal_string_for_key(*version, source, "name")?,
                    "served": literal_bool_for_key(*version, source, "served"),
                    "schema": { "openAPIV3Schema": schema },
                }))
            })
            .collect::<Option<Vec<_>>>()?;
        spec_json["versions"] = Value::Array(versions);
    } else {
        let validation = mapping_value(spec, source, "validation")?;
        spec_json["version"] = Value::String(literal_string_for_key(spec, source, "version")?);
        spec_json["validation"] = json!({
            "openAPIV3Schema": mapping_value(validation, source, "openAPIV3Schema")
                .and_then(|schema| literal_json_from_node(schema, source))?,
        });
    }

    Some(json!({
        "apiVersion": literal_string_for_key(node, source, "apiVersion")?,
        "kind": literal_string_for_key(node, source, "kind")?,
        "spec": spec_json,
    }))
}

fn mapping_value<'tree>(
    node: tree_sitter::Node<'tree>,
    source: &str,
    key: &str,
) -> Option<tree_sitter::Node<'tree>> {
    for pair in mapping_pairs(mapping_node(node)?) {
        let pair_key = pair.child_by_field_name("key")?;
        if literal_string_from_node(pair_key, source).as_deref() == Some(key) {
            return pair
                .child_by_field_name("value")
                .map(unwrap_yaml_value_node);
        }
    }
    None
}

/// Named `key: value` pair nodes of a mapping node; block and flow
/// mappings name their pair kind differently.
fn mapping_pairs(mapping: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let pair_kind = match mapping.kind() {
        "block_mapping" => "block_mapping_pair",
        "flow_mapping" => "flow_pair",
        _ => return Vec::new(),
    };
    let mut cursor = mapping.walk();
    mapping
        .children(&mut cursor)
        .filter(|pair| pair.is_named() && pair.kind() == pair_kind)
        .collect()
}

fn sequence_items(node: tree_sitter::Node<'_>) -> Option<Vec<tree_sitter::Node<'_>>> {
    let node = unwrap_yaml_value_node(node);
    if !matches!(node.kind(), "block_sequence" | "flow_sequence") {
        return None;
    }
    let mut items = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if matches!(
            child.kind(),
            "block_sequence_item" | "flow_node" | "flow_pair"
        ) {
            items.push(unwrap_yaml_value_node(child));
        }
    }
    Some(items)
}

fn literal_string_for_key(node: tree_sitter::Node<'_>, source: &str, key: &str) -> Option<String> {
    mapping_value(node, source, key).and_then(|value| literal_string_from_node(value, source))
}

fn literal_bool_for_key(node: tree_sitter::Node<'_>, source: &str, key: &str) -> Option<bool> {
    mapping_value(node, source, key)
        .and_then(|value| literal_json_from_node(value, source))?
        .as_bool()
}

fn literal_string_from_node(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    literal_json_from_node(node, source)?
        .as_str()
        .map(std::string::ToString::to_string)
}

fn literal_json_from_node(node: tree_sitter::Node<'_>, source: &str) -> Option<Value> {
    let node = unwrap_yaml_value_node(node);
    match node.kind() {
        "stream" | "document" | "block_node" | "flow_node" => {
            let mut cursor = node.walk();
            let children = node
                .named_children(&mut cursor)
                .filter(|child| {
                    !matches!(
                        child.kind(),
                        "reserved_directive" | "tag_directive" | "yaml_directive" | "comment"
                    )
                })
                .collect::<Vec<_>>();
            if children.len() == 1 {
                literal_json_from_node(children[0], source)
            } else {
                None
            }
        }
        "block_mapping" | "flow_mapping" => {
            let mut object = serde_json::Map::new();
            for pair in mapping_pairs(node) {
                let key = literal_string_from_node(pair.child_by_field_name("key")?, source)?;
                let value = pair
                    .child_by_field_name("value")
                    .map(|value| literal_json_from_node(value, source))
                    .unwrap_or(Some(Value::Null))?;
                object.insert(key, value);
            }
            Some(Value::Object(object))
        }
        "block_sequence" | "flow_sequence" => sequence_items(node)?
            .into_iter()
            .map(|item| literal_json_from_node(item, source))
            .collect::<Option<Vec<_>>>()
            .map(Value::Array),
        "helm_template" => None,
        _ => {
            let text = node.utf8_text(source.as_bytes()).ok()?.trim();
            if text.contains("{{") || text.contains("}}") {
                return None;
            }
            let yaml_value = serde_yaml::from_str::<serde_yaml::Value>(text).ok()?;
            serde_json::to_value(yaml_value).ok()
        }
    }
}

fn mapping_node(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let node = unwrap_yaml_value_node(node);
    match node.kind() {
        "block_mapping" | "flow_mapping" => Some(node),
        "stream" | "document" => node.named_child(0).and_then(mapping_node),
        _ => None,
    }
}

fn unwrap_yaml_value_node(node: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    if matches!(
        node.kind(),
        "block_node" | "flow_node" | "block_sequence_item"
    ) && let Some(child) = node.named_child(0)
    {
        return unwrap_yaml_value_node(child);
    }
    node
}

#[cfg(test)]
#[path = "tests/local_crd_projection.rs"]
mod tests;
