use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use crate::merge::merge_schema_list;
use crate::{empty_schema, object_schema, unknown_object_schema};
use helm_schema_k8s::type_schema;

pub(crate) struct ValuesYamlPathInfo {
    pub(crate) schema: Value,
    pub(crate) is_explicit_null: bool,
    pub(crate) is_empty_string: bool,
    pub(crate) is_empty_map: bool,
    pub(crate) is_mapping: bool,
}

pub(crate) struct ValuePathCaches {
    pub(crate) path_segments: BTreeMap<String, Vec<String>>,
    pub(crate) values_yaml: BTreeMap<String, ValuesYamlPathInfo>,
}

#[tracing::instrument(skip_all)]
pub(crate) fn build_value_path_caches(
    values_yaml_doc: &YamlValue,
    referenced_value_paths: &BTreeSet<String>,
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

fn schema_from_yaml_value(value: &YamlValue) -> Value {
    match value {
        YamlValue::Null | YamlValue::Tagged(_) => empty_schema(),
        YamlValue::Bool(_) => type_schema("boolean"),
        YamlValue::Number(number) => {
            if number.as_i64().is_some() || number.as_u64().is_some() {
                type_schema("integer")
            } else {
                type_schema("number")
            }
        }
        YamlValue::String(_) => type_schema("string"),
        YamlValue::Sequence(sequence) => {
            let items = if sequence.is_empty() {
                empty_schema()
            } else {
                merge_schema_list(sequence.iter().map(schema_from_yaml_value).collect())
            };
            Value::Object(
                [
                    ("type".to_string(), Value::String("array".to_string())),
                    ("items".to_string(), items),
                ]
                .into_iter()
                .collect(),
            )
        }
        YamlValue::Mapping(mapping) => {
            if mapping.is_empty() {
                return unknown_object_schema();
            }
            let mut properties = Map::new();
            for (key, value) in mapping {
                let Some(key) = key.as_str() else {
                    continue;
                };
                properties.insert(key.to_string(), schema_from_yaml_value(value));
            }
            object_schema(properties)
        }
    }
}
