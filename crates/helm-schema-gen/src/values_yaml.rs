use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::merge::merge_schema_list;
use crate::schema_model::{empty_schema, is_empty_schema};
use crate::schema_node::SchemaNode;
use crate::schema_tree::unknown_object_schema;

pub(crate) struct ValuesYamlPathInfo {
    pub(crate) schema: Value,
    pub(crate) is_explicit_null: bool,
    pub(crate) is_empty_string: bool,
    pub(crate) is_empty_map: bool,
    pub(crate) is_mapping: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ValuesYamlPathFacts {
    pub(crate) has_no_schema_evidence: bool,
    pub(crate) is_explicit_null: bool,
    pub(crate) is_empty_string: bool,
    pub(crate) is_empty_map: bool,
    pub(crate) is_mapping: bool,
}

impl ValuesYamlPathFacts {
    pub(crate) fn absent() -> Self {
        Self {
            has_no_schema_evidence: true,
            ..Self::default()
        }
    }
}

impl ValuesYamlPathInfo {
    pub(crate) fn facts(&self) -> ValuesYamlPathFacts {
        ValuesYamlPathFacts {
            has_no_schema_evidence: is_empty_schema(&self.schema),
            is_explicit_null: self.is_explicit_null,
            is_empty_string: self.is_empty_string,
            is_empty_map: self.is_empty_map,
            is_mapping: self.is_mapping,
        }
    }
}

pub(crate) struct ValuePathCaches {
    pub(crate) path_segments: BTreeMap<String, Vec<String>>,
    pub(crate) values_yaml: BTreeMap<String, ValuesYamlPathInfo>,
}

#[tracing::instrument(skip_all)]
pub(crate) fn build_value_path_caches(
    values_yaml_doc: &YamlValue,
    referenced_value_paths: &BTreeSet<String>,
    pruned_parent_value_paths: &BTreeSet<String>,
) -> ValuePathCaches {
    let path_segments: BTreeMap<String, Vec<String>> = referenced_value_paths
        .iter()
        .map(|path| {
            (
                path.clone(),
                path.split('.')
                    .filter(|segment| !segment.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect(),
            )
        })
        .collect();

    let values_yaml = path_segments
        .iter()
        .filter_map(|(path, segments)| {
            lookup_values_yaml_path_info(values_yaml_doc, segments)
                .map(|mut path_info| {
                    if pruned_parent_value_paths.contains(path) {
                        prune_referenced_descendant_schemas(
                            &mut path_info.schema,
                            path,
                            referenced_value_paths,
                        );
                    }
                    path_info
                })
                .map(|path_info| (path.clone(), path_info))
        })
        .collect();

    ValuePathCaches {
        path_segments,
        values_yaml,
    }
}

fn lookup_values_yaml_path_info(
    doc: &YamlValue,
    path_segments: &[String],
) -> Option<ValuesYamlPathInfo> {
    if path_segments.is_empty() {
        return None;
    }

    let values = lookup_values_yaml_values(doc, path_segments)?;
    if values.is_empty() {
        return None;
    }

    let schema = merge_schema_list(values.iter().copied().map(schema_from_yaml_value).collect());
    let is_explicit_null = values.len() == 1 && matches!(values[0], YamlValue::Null);
    let is_empty_string = values
        .iter()
        .any(|value| matches!(value, YamlValue::String(value) if value.is_empty()));
    let is_empty_map = values
        .iter()
        .all(|value| matches!(value, YamlValue::Mapping(map) if map.is_empty()));
    let is_mapping = values
        .iter()
        .all(|value| matches!(value, YamlValue::Mapping(_)));

    Some(ValuesYamlPathInfo {
        schema,
        is_explicit_null,
        is_empty_string,
        is_empty_map,
        is_mapping,
    })
}

fn lookup_values_yaml_values<'a>(
    doc: &'a YamlValue,
    path_segments: &[String],
) -> Option<Vec<&'a YamlValue>> {
    if path_segments.is_empty() {
        return Some(vec![doc]);
    }

    let head = path_segments[0].as_str();
    let tail = &path_segments[1..];

    match doc {
        YamlValue::Mapping(map) => {
            let key = YamlValue::String(head.to_string());
            let next = map.get(&key)?;
            lookup_values_yaml_values(next, tail)
        }
        YamlValue::Sequence(sequence) if head == "*" => {
            let mut out: Vec<&'a YamlValue> = Vec::new();
            for item in sequence {
                if let Some(mut child) = lookup_values_yaml_values(item, tail) {
                    out.append(&mut child);
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

fn prune_referenced_descendant_schemas(
    schema: &mut Value,
    value_path: &str,
    referenced_value_paths: &BTreeSet<String>,
) {
    let descendant_prefix = format!("{value_path}.");
    let mut relative_paths_to_prune = BTreeSet::new();
    for descendant in referenced_value_paths {
        let Some(relative_path) = descendant.strip_prefix(&descendant_prefix) else {
            continue;
        };
        let relative_segments: Vec<String> = relative_path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
        if relative_segments.is_empty() {
            continue;
        }
        relative_paths_to_prune.insert(shortest_referenced_relative_path(
            value_path,
            &relative_segments,
            referenced_value_paths,
        ));
    }

    for relative_segments in relative_paths_to_prune {
        let relative_segments: Vec<&str> = relative_segments
            .iter()
            .map(std::string::String::as_str)
            .collect();
        prune_schema_at_relative_path(schema, &relative_segments);
    }
}

fn shortest_referenced_relative_path(
    value_path: &str,
    relative_segments: &[String],
    referenced_value_paths: &BTreeSet<String>,
) -> Vec<String> {
    let mut prefix = Vec::new();
    for segment in relative_segments {
        prefix.push(segment.clone());
        let candidate_path = format!("{value_path}.{}", prefix.join("."));
        if referenced_value_paths.contains(&candidate_path) {
            return prefix;
        }
    }
    relative_segments.to_vec()
}

fn prune_schema_at_relative_path(schema: &mut Value, relative_segments: &[&str]) {
    let Some((head, tail)) = relative_segments.split_first() else {
        return;
    };
    let Value::Object(object) = schema else {
        return;
    };

    if *head == "*" {
        if let Some(items) = object.get_mut("items") {
            if tail.is_empty() {
                *items = empty_schema();
            } else {
                prune_schema_at_relative_path(items, tail);
            }
        }
        return;
    }

    let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    if tail.is_empty() {
        properties.remove(*head);
        return;
    }

    if let Some(child) = properties.get_mut(*head) {
        prune_schema_at_relative_path(child, tail);
    }
}

fn schema_from_yaml_value(value: &YamlValue) -> Value {
    schema_node_from_yaml_value(value).into_value()
}

fn schema_node_from_yaml_value(value: &YamlValue) -> SchemaNode {
    match value {
        YamlValue::Null | YamlValue::Tagged(_) => SchemaNode::empty(),
        YamlValue::Bool(_) => SchemaNode::type_named("boolean"),
        YamlValue::Number(number) => {
            if number.as_i64().is_some() || number.as_u64().is_some() {
                SchemaNode::type_named("integer")
            } else {
                SchemaNode::type_named("number")
            }
        }
        YamlValue::String(_) => SchemaNode::type_named("string"),
        YamlValue::Sequence(sequence) => {
            let items = if sequence.is_empty() {
                empty_schema()
            } else {
                merge_schema_list(sequence.iter().map(schema_from_yaml_value).collect())
            };
            SchemaNode::array().items(SchemaNode::foreign(items))
        }
        YamlValue::Mapping(mapping) => {
            if mapping.is_empty() {
                return SchemaNode::foreign(unknown_object_schema());
            }
            let mut schema = SchemaNode::closed_object();
            for (key, value) in mapping {
                let Some(key) = key.as_str() else {
                    continue;
                };
                schema = schema.property(key.to_string(), schema_node_from_yaml_value(value));
            }
            schema
        }
    }
}
