use helm_schema_core::GuardValue;
use serde_json::{Number, Value};

use crate::merge::{merge_two_schemas, union_schema_list};
use crate::schema_node::{JsonSchemaType, SchemaNode};

pub(crate) fn type_schema(ty: &str) -> Value {
    SchemaNode::type_named(ty).into_value()
}

pub(crate) fn guard_value_to_json(value: &GuardValue) -> Option<Value> {
    match value {
        GuardValue::String(value) => Some(Value::String(value.clone())),
        GuardValue::Bool(value) => Some(Value::Bool(*value)),
        GuardValue::Int(value) => Some(Value::Number((*value).into())),
        GuardValue::Float(value) => value
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number),
        GuardValue::Null => Some(Value::Null),
    }
}

pub(crate) fn schema_type(value: &Value) -> Option<&str> {
    value.as_object()?.get("type")?.as_str()
}

/// The domain of a scalar string slot: Go template printing renders every
/// scalar into flag/annotation splices (`-v={{ x }}`), so a declared
/// scalar's type widens to this union there.
pub(crate) fn scalar_union_schema() -> Value {
    serde_json::json!({ "type": ["boolean", "integer", "number", "string"] })
}

pub(crate) fn is_scalar_schema(value: &Value) -> bool {
    matches!(
        schema_type(value),
        Some("string" | "integer" | "number" | "boolean")
    )
}

pub(crate) fn is_scalar_like_schema(value: &Value) -> bool {
    if is_scalar_schema(value) {
        return true;
    }

    let Some(object) = value.as_object() else {
        return false;
    };

    if let Some(Value::Array(values)) = object.get("enum") {
        return !values.is_empty()
            && values.iter().all(|value| {
                matches!(
                    value,
                    Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
                )
            });
    }

    if let Some(Value::Array(types)) = object.get("type") {
        return types.iter().all(|value| {
            matches!(
                value.as_str(),
                Some("string" | "number" | "integer" | "boolean" | "null")
            )
        });
    }

    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(variants)) = object.get(key) {
            return !variants.is_empty() && variants.iter().all(is_scalar_like_schema);
        }
    }

    false
}

pub(crate) fn is_object_or_array_schema(value: &Value) -> bool {
    matches!(schema_type(value), Some("object" | "array"))
}

pub(crate) fn is_fixed_object_schema(value: &Value) -> bool {
    if schema_type(value) != Some("object") {
        return false;
    }
    let Some(object) = value.as_object() else {
        return false;
    };
    object
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|properties| !properties.is_empty())
        && object.get("additionalProperties") == Some(&Value::Bool(false))
}

/// An object schema carrying declared property structure, open or closed:
/// the shape `values.yaml` mappings lower to. Declared defaults document
/// keys without bounding them, so consumers keying decisions on "this is a
/// declared map shape" must accept the open form.
pub(crate) fn is_declared_object_schema(value: &Value) -> bool {
    if schema_type(value) != Some("object") {
        return false;
    }
    let Some(object) = value.as_object() else {
        return false;
    };
    let unconstrained_additional = match object.get("additionalProperties") {
        None | Some(Value::Bool(false)) => true,
        Some(Value::Object(map)) => map.is_empty(),
        Some(_) => false,
    };
    object
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|properties| !properties.is_empty())
        && unconstrained_additional
}

pub(crate) fn is_open_string_map_schema(value: &Value) -> bool {
    if schema_type(value) != Some("object") {
        return false;
    }
    let Some(object) = value.as_object() else {
        return false;
    };
    matches!(
        object.get("additionalProperties"),
        Some(Value::Object(map))
            if map.get("type").and_then(Value::as_str) == Some("string")
    )
}

pub(crate) fn schema_allows_type(schema: &Value, expected_type: &str) -> bool {
    if let Some(schema_type) = schema_type(schema) {
        return schema_type == expected_type;
    }

    let Some(object) = schema.as_object() else {
        return false;
    };

    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Array(variants)) = object.get(key)
            && variants
                .iter()
                .any(|variant| schema_allows_type(variant, expected_type))
        {
            return true;
        }
    }

    false
}

pub(crate) fn add_null_schema(schema: Value) -> Value {
    if schema.get("anyOf").and_then(Value::as_array).is_some()
        || schema.get("oneOf").and_then(Value::as_array).is_some()
    {
        union_schema_list(vec![schema, type_schema("null")])
    } else {
        merge_two_schemas(schema, type_schema("null"))
    }
}

pub(crate) fn empty_string_schema() -> Value {
    SchemaNode::typed(JsonSchemaType::String)
        .typed_keyword("enum", Value::Array(vec![Value::String(String::new())]))
        .into_value()
}

pub(crate) fn schema_permits_empty_string(schema: &Value) -> bool {
    if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
        return variants.iter().any(schema_permits_empty_string);
    }
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        return variants.iter().any(schema_permits_empty_string);
    }

    let Some(object) = schema.as_object() else {
        return false;
    };
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        return values.iter().any(|value| value.as_str() == Some(""));
    }
    if object.get("pattern").is_some() {
        return false;
    }

    let type_allows_string = object.get("type").and_then(Value::as_str) == Some("string")
        || object
            .get("type")
            .and_then(Value::as_array)
            .is_some_and(|types| types.iter().any(|value| value.as_str() == Some("string")));
    type_allows_string
        && object
            .get("minLength")
            .and_then(Value::as_u64)
            .is_none_or(|min_length| min_length == 0)
}

pub(crate) fn is_empty_schema(value: &Value) -> bool {
    value.as_object().is_some_and(serde_json::Map::is_empty)
}

pub(crate) fn is_annotation_keyword(key: &str) -> bool {
    matches!(
        key,
        "description" | "title" | "default" | "examples" | "deprecated" | "readOnly" | "writeOnly"
    )
}

pub(crate) fn empty_schema() -> Value {
    SchemaNode::empty().into_value()
}

pub(crate) fn exact_empty_object_schema() -> Value {
    SchemaNode::closed_object().max_properties(0).into_value()
}
