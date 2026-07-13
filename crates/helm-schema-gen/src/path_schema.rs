use serde_json::Value;

use crate::merge::{merge_schema_list, union_schema_list};
use crate::schema_model::{
    add_null_schema, empty_schema, exact_empty_object_schema, is_declared_object_schema,
    schema_allows_type, type_schema,
};
use crate::schema_node::SchemaNode;

pub(crate) fn merge_explicit_empty_placeholder(
    schema: Value,
    is_empty_map: bool,
    collection_shape_known: bool,
    preserve_exact_off_state: bool,
) -> Value {
    if is_empty_map {
        if crate::schema_model::is_empty_schema(&schema) {
            // No merged shape evidence. When descendant rows describe the
            // collection elsewhere (a list-ranged source), the declared-empty
            // default is purely the off-state; otherwise the chart iterates
            // user-supplied entries and the map stays open.
            return if collection_shape_known {
                exact_empty_object_schema()
            } else {
                SchemaNode::unknown_object().into_value()
            };
        }
        if preserve_exact_off_state {
            return union_schema_list(vec![schema, exact_empty_object_schema()]);
        }
        if schema_accepts_empty_object(&schema) {
            return stamp_explicit_map_openness(schema);
        }
        union_schema_list(vec![schema, exact_empty_object_schema()])
    } else {
        schema
    }
}

/// Makes a no-opinion `additionalProperties` explicit on a user-populated
/// map's schema. Semantically a no-op (an absent `additionalProperties`
/// already accepts everything), but the schema tree reads the explicit form
/// as openness evidence: without it, a later literal member read (e.g. a
/// guard probing one key) closes the map when its descendant fragment merges
/// into the slot.
pub(crate) fn stamp_explicit_map_openness(mut schema: Value) -> Value {
    if let Some(object) = schema.as_object_mut()
        && object.get("type").and_then(Value::as_str) == Some("object")
        && !object.contains_key("additionalProperties")
    {
        object.insert(
            "additionalProperties".to_string(),
            crate::schema_model::empty_schema(),
        );
    }
    schema
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
    if !is_declared_object_schema(&schema) {
        return schema;
    }
    let Some(object) = schema.as_object() else {
        return schema;
    };
    let Some(properties) = object.get("properties").and_then(Value::as_object) else {
        return schema;
    };

    let merged_value_schema = merge_schema_list(properties.values().cloned().collect());
    properties
        .iter()
        .fold(
            SchemaNode::object()
                .with_empty_properties()
                .with_additional_properties(SchemaNode::foreign(merged_value_schema)),
            |schema, (key, value)| schema.property(key.clone(), SchemaNode::foreign(value.clone())),
        )
        .into_value()
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

            let schema_type = object.get("type").and_then(Value::as_str);
            let is_array = schema_type == Some("array");
            let is_scalar = matches!(
                schema_type,
                Some("boolean" | "integer" | "number" | "string")
            );
            let is_object = schema_type == Some("object");

            let schema = if is_array {
                let items = object
                    .remove("items")
                    .map(|items| open_fragment_values_schema_inner(items, false))
                    .unwrap_or_else(empty_schema);
                SchemaNode::array()
                    .items(SchemaNode::foreign(items))
                    .into_value()
            } else if is_object {
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
                // A fragment splices the WHOLE subtree through: undeclared
                // members are passthrough config the chart renders verbatim,
                // so they stay unconstrained. Typing them as the merge of
                // the declared property schemas rejected legitimate keys
                // whose shape differs from the declared ones.
                properties
                    .into_iter()
                    .fold(
                        SchemaNode::object()
                            .with_additional_properties(SchemaNode::foreign(empty_schema())),
                        |schema, (key, value)| schema.property(key, SchemaNode::foreign(value)),
                    )
                    .into_value()
            } else {
                Value::Object(object)
            };

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
