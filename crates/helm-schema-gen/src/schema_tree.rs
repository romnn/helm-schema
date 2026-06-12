use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::merge::{merge_two_schemas, union_schema_list};
use crate::{is_empty_schema, schema_type};

const MAP_WILDCARD_SEGMENT: &str = "__any__";

pub(crate) fn object_schema(properties: Map<String, Value>) -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            ("properties".to_string(), Value::Object(properties)),
            ("additionalProperties".to_string(), Value::Bool(false)),
        ]
        .into_iter()
        .collect(),
    )
}

pub(crate) fn unknown_object_schema() -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            (
                "additionalProperties".to_string(),
                Value::Object(Map::new()),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

pub(crate) fn insert_schema_at_path_segments(
    root: &mut Value,
    path_segments: &[String],
    leaf: Value,
) {
    if path_segments.is_empty() {
        return;
    }
    insert_schema_at_parts(root, path_segments, leaf);
}

pub(crate) fn apply_values_descriptions(root: &mut Value, descriptions: &BTreeMap<String, String>) {
    for (path, description) in descriptions {
        let path_segments: Vec<String> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
        apply_description_at_path_segments(root, &path_segments, description);
    }
}

fn apply_description_at_path_segments(
    node: &mut Value,
    path_segments: &[String],
    description: &str,
) {
    if path_segments.is_empty() {
        set_schema_description(node, description);
        return;
    }

    let Some((head, tail)) = path_segments.split_first() else {
        return;
    };

    let Value::Object(obj) = node else {
        return;
    };

    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(variants)) = obj.get_mut(key) {
            for variant in variants {
                apply_description_at_path_segments(variant, path_segments, description);
            }
        }
    }

    if head == "*" {
        if let Some(items) = obj.get_mut("items") {
            apply_description_at_path_segments(items, tail, description);
        }
        return;
    }

    if head == MAP_WILDCARD_SEGMENT {
        if let Some(additional_properties) = obj.get_mut("additionalProperties") {
            apply_description_at_path_segments(additional_properties, tail, description);
        }
        return;
    }

    let Some(properties) = obj.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(child) = properties.get_mut(head) else {
        return;
    };
    apply_description_at_path_segments(child, tail, description);
}

fn set_schema_description(node: &mut Value, description: &str) {
    if description.trim().is_empty() {
        return;
    }

    if let Value::Object(obj) = node {
        obj.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
}

fn ensure_object_schema(value: &mut Value) {
    match value {
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str) != Some("object") {
                obj.insert("type".to_string(), Value::String("object".to_string()));
            }
            obj.entry("properties".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if obj.get("properties").and_then(Value::as_object).is_none() {
                obj.insert("properties".to_string(), Value::Object(Map::new()));
            }
            obj.entry("additionalProperties".to_string())
                .or_insert(Value::Bool(false));

            let has_structure = obj
                .get("properties")
                .and_then(Value::as_object)
                .is_some_and(|map| !map.is_empty())
                || obj
                    .get("patternProperties")
                    .and_then(Value::as_object)
                    .is_some_and(|map| !map.is_empty())
                || obj
                    .get("required")
                    .and_then(Value::as_array)
                    .is_some_and(|array| !array.is_empty());

            let additional_properties_is_empty_schema = obj
                .get("additionalProperties")
                .and_then(Value::as_object)
                .is_some_and(serde_json::Map::is_empty);

            if has_structure && additional_properties_is_empty_schema {
                obj.insert("additionalProperties".to_string(), Value::Bool(false));
            }
        }
        _ => {
            *value = object_schema(Map::new());
        }
    }
}

fn ensure_array_schema(value: &mut Value) {
    match value {
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str) != Some("array") {
                obj.insert("type".to_string(), Value::String("array".to_string()));
            }
            obj.entry("items".to_string()).or_insert(Value::Null);
        }
        _ => {
            *value = Value::Object(
                [
                    ("type".to_string(), Value::String("array".to_string())),
                    ("items".to_string(), Value::Null),
                ]
                .into_iter()
                .collect(),
            );
        }
    }
}

fn ensure_items_schema(array_schema: &mut Value) -> &mut Value {
    array_schema
        .as_object_mut()
        .and_then(|object| object.get_mut("items"))
        .expect("array schema must have items")
}

fn clear_exact_empty_constraint_for_descendant(node: &mut Value) {
    if let Value::Object(obj) = node
        && obj.get("maxProperties").and_then(Value::as_u64) == Some(0)
    {
        obj.remove("maxProperties");
    }
}

fn is_object_like_schema(value: &Value) -> bool {
    match schema_type(value) {
        Some("object") => true,
        Some(_) => false,
        None => value.as_object().is_some_and(|object| {
            object.contains_key("properties")
                || object.contains_key("additionalProperties")
                || object.contains_key("patternProperties")
                || object.contains_key("required")
        }),
    }
}

fn is_array_like_schema(value: &Value) -> bool {
    match schema_type(value) {
        Some("array") => true,
        Some(_) => false,
        None => value
            .as_object()
            .is_some_and(|object| object.contains_key("items")),
    }
}

fn union_key(obj: &Map<String, Value>) -> Option<&'static str> {
    if obj.get("anyOf").and_then(Value::as_array).is_some() {
        Some("anyOf")
    } else if obj.get("oneOf").and_then(Value::as_array).is_some() {
        Some("oneOf")
    } else {
        None
    }
}

fn take_union_variants(obj: &mut Map<String, Value>, key: &str) -> Option<Vec<Value>> {
    let Value::Array(variants) = obj.remove(key).unwrap_or_else(|| Value::Array(Vec::new())) else {
        return None;
    };
    Some(variants)
}

fn push_union_structural_constraints_down(obj: &mut Map<String, Value>, variants: &mut [Value]) {
    let structural_keys = [
        "type",
        "properties",
        "additionalProperties",
        "patternProperties",
        "required",
        "items",
    ];
    let mut structural = Map::new();
    for key in structural_keys {
        if let Some(value) = obj.remove(key) {
            structural.insert(key.to_string(), value);
        }
    }

    if structural.is_empty() {
        return;
    }

    let structural_schema = Value::Object(structural);
    for variant in variants {
        let compatible = if is_array_like_schema(&structural_schema) {
            is_array_like_schema(variant)
        } else {
            is_object_like_schema(variant)
        };

        if compatible {
            let existing = std::mem::replace(variant, Value::Null);
            *variant = merge_two_schemas(existing, structural_schema.clone());
        }
    }
}

fn insert_schema_into_union_variants(
    variants: &mut [Value],
    path_segments: &[String],
    leaf: &Value,
) -> bool {
    let head = path_segments[0].as_str();
    let mut touched = false;
    for variant in variants {
        let compatible = if head == "*" {
            is_array_like_schema(variant)
        } else {
            is_object_like_schema(variant)
        };

        if compatible {
            insert_schema_at_parts(variant, path_segments, leaf.clone());
            touched = true;
        }
    }
    touched
}

fn new_union_variant_for_head(head: &str) -> Value {
    if head == "*" {
        Value::Object(
            [
                ("type".to_string(), Value::String("array".to_string())),
                ("items".to_string(), Value::Null),
            ]
            .into_iter()
            .collect(),
        )
    } else {
        object_schema(Map::new())
    }
}

fn insert_schema_at_union(
    obj: &mut Map<String, Value>,
    key: &'static str,
    path_segments: &[String],
    leaf: Value,
) {
    let Some(mut variants) = take_union_variants(obj, key) else {
        return;
    };

    push_union_structural_constraints_down(obj, &mut variants);

    let touched = insert_schema_into_union_variants(&mut variants, path_segments, &leaf);
    if !touched {
        let mut new_variant = new_union_variant_for_head(path_segments[0].as_str());
        insert_schema_at_parts(&mut new_variant, path_segments, leaf);
        variants.push(new_variant);
    }

    obj.insert(key.to_string(), Value::Array(variants));
}

fn insert_schema_at_parts(node: &mut Value, path_segments: &[String], leaf: Value) {
    if path_segments.is_empty() {
        return;
    }

    // Union-aware insertion updates the compatible variant instead of forcing
    // the union node itself into an object or array schema.
    if let Value::Object(obj) = node
        && let Some(key) = union_key(obj)
    {
        insert_schema_at_union(obj, key, path_segments, leaf);
        return;
    }

    if path_segments[0] == MAP_WILDCARD_SEGMENT {
        if path_segments.len() > 1 {
            clear_exact_empty_constraint_for_descendant(node);
        }
        ensure_object_schema(node);
        let obj = node.as_object_mut().expect("object schema");
        let additional_properties = obj
            .entry("additionalProperties")
            .or_insert_with(|| Value::Object(Map::new()));
        if additional_properties.as_bool() == Some(false) {
            *additional_properties = Value::Object(Map::new());
        }
        if path_segments.len() == 1 {
            let existing = std::mem::replace(additional_properties, Value::Null);
            *additional_properties = match existing {
                Value::Null => leaf,
                other => merge_two_schemas(other, leaf),
            };
        } else {
            clear_exact_empty_constraint_for_descendant(additional_properties);
            insert_schema_at_parts(additional_properties, &path_segments[1..], leaf);
        }
        return;
    }

    if path_segments[0] == "*" {
        if !is_empty_schema(node) && !is_array_like_schema(node) {
            let existing = std::mem::replace(node, Value::Null);
            let mut array_variant = new_union_variant_for_head("*");
            insert_schema_at_parts(&mut array_variant, path_segments, leaf);
            *node = union_schema_list(vec![existing, array_variant]);
            return;
        }
        ensure_array_schema(node);
        let items = ensure_items_schema(node);
        if path_segments.len() == 1 {
            let existing = std::mem::replace(items, Value::Null);
            *items = match existing {
                Value::Null => leaf,
                other => merge_two_schemas(other, leaf),
            };
        } else {
            insert_schema_at_parts(items, &path_segments[1..], leaf);
        }
        return;
    }

    if path_segments.len() > 1 {
        clear_exact_empty_constraint_for_descendant(node);
    }
    ensure_object_schema(node);
    let properties = node
        .as_object_mut()
        .and_then(|object| object.get_mut("properties"))
        .and_then(Value::as_object_mut)
        .expect("object schema must have properties");

    if path_segments.len() == 1 {
        let key = path_segments[0].clone();
        match properties.entry(key) {
            serde_json::map::Entry::Vacant(entry) => {
                entry.insert(leaf);
            }
            serde_json::map::Entry::Occupied(mut entry) => {
                let existing = std::mem::replace(entry.get_mut(), Value::Null);
                *entry.get_mut() = merge_two_schemas(existing, leaf);
            }
        }
        return;
    }

    let key = path_segments[0].clone();
    let child = properties.entry(key).or_insert_with(|| {
        if path_segments.get(1).is_some_and(|segment| segment == "*") {
            new_union_variant_for_head("*")
        } else {
            object_schema(Map::new())
        }
    });
    if path_segments.get(1).is_none_or(|segment| segment != "*") {
        clear_exact_empty_constraint_for_descendant(child);
    }
    insert_schema_at_parts(child, &path_segments[1..], leaf);
}
