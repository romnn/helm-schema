use serde_json::Value;

use helm_schema_k8s::type_schema;

use crate::merge::{merge_schema_list, union_schema_list};
use crate::schema_model::{
    add_null_schema, empty_schema, exact_empty_object_schema, is_fixed_object_schema,
    schema_allows_type,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct EmptyMapPlaceholderUse {
    pub(crate) is_empty_map: bool,
    pub(crate) is_ranged_source: bool,
    pub(crate) has_self_range_guard_render_use: bool,
    pub(crate) has_render_use: bool,
    pub(crate) all_render_uses_self_guarded: bool,
    pub(crate) used_as_fragment: bool,
}

pub(crate) fn empty_map_placeholder_has_structural_object_use(
    provider_schema: &Value,
    placeholder_use: EmptyMapPlaceholderUse,
) -> bool {
    placeholder_use.is_empty_map
        && (placeholder_use.is_ranged_source
            || placeholder_use.has_self_range_guard_render_use
            || (schema_allows_type(provider_schema, "object")
                && (placeholder_use.used_as_fragment
                    || (placeholder_use.has_render_use
                        && placeholder_use.all_render_uses_self_guarded))))
}

pub(crate) fn merge_explicit_empty_placeholder(schema: Value, is_empty_map: bool) -> Value {
    if is_empty_map {
        if schema_accepts_empty_object(&schema) {
            return schema;
        }
        union_schema_list(vec![schema, exact_empty_object_schema()])
    } else {
        schema
    }
}

fn schema_accepts_empty_object(schema: &Value) -> bool {
    if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
        return variants.iter().any(schema_accepts_empty_object);
    }

    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        return variants.iter().any(schema_accepts_empty_object);
    }

    if !schema_allows_type(schema, "object") {
        return false;
    }

    let required_is_empty = schema
        .get("required")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty);
    let min_properties_allows_empty = schema
        .get("minProperties")
        .and_then(Value::as_u64)
        .is_none_or(|min| min == 0);

    required_is_empty && min_properties_allows_empty
}

pub(crate) fn generalize_fixed_object_schema_to_open_map(schema: Value) -> Value {
    if !is_fixed_object_schema(&schema) {
        return schema;
    }
    let Some(object) = schema.as_object() else {
        return schema;
    };
    let Some(properties) = object.get("properties").and_then(Value::as_object) else {
        return schema;
    };

    let merged_value_schema = merge_schema_list(properties.values().cloned().collect());
    let mut out = object.clone();
    out.insert("additionalProperties".to_string(), merged_value_schema);
    Value::Object(out)
}

pub(crate) fn open_fragment_values_schema(schema: Value) -> Value {
    open_fragment_values_schema_inner(schema, true)
}

fn open_fragment_values_schema_inner(schema: Value, widen_self: bool) -> Value {
    match schema {
        Value::Object(mut object) => {
            if let Some(Value::Array(variants)) = object.remove("anyOf") {
                object.insert(
                    "anyOf".to_string(),
                    Value::Array(
                        variants
                            .into_iter()
                            .map(|variant| open_fragment_values_schema_inner(variant, widen_self))
                            .collect(),
                    ),
                );
                return Value::Object(object);
            }
            if let Some(Value::Array(variants)) = object.remove("oneOf") {
                object.insert(
                    "oneOf".to_string(),
                    Value::Array(
                        variants
                            .into_iter()
                            .map(|variant| open_fragment_values_schema_inner(variant, widen_self))
                            .collect(),
                    ),
                );
                return Value::Object(object);
            }

            if let Some(items) = object.remove("items") {
                object.insert(
                    "items".to_string(),
                    open_fragment_values_schema_inner(items, false),
                );
            }

            let schema_type = object.get("type").and_then(Value::as_str);
            let is_array = schema_type == Some("array");
            let is_scalar = matches!(
                schema_type,
                Some("boolean" | "integer" | "number" | "string")
            );
            let is_object = schema_type == Some("object");
            if is_object {
                let mut properties = object
                    .remove("properties")
                    .and_then(|value| match value {
                        Value::Object(properties) => Some(properties),
                        _ => None,
                    })
                    .unwrap_or_default();
                for value in properties.values_mut() {
                    *value = open_fragment_values_schema_inner(value.take(), false);
                }
                let additional_properties = if properties.is_empty() {
                    empty_schema()
                } else {
                    merge_schema_list(properties.values().cloned().collect())
                };
                if !properties.is_empty() {
                    object.insert("properties".to_string(), Value::Object(properties));
                }
                object.insert("additionalProperties".to_string(), additional_properties);
            }

            let schema = Value::Object(object);
            if widen_self && is_array {
                union_schema_list(vec![schema, type_schema("null"), type_schema("string")])
            } else if widen_self && is_scalar {
                add_null_schema(schema)
            } else {
                schema
            }
        }
        other => other,
    }
}
