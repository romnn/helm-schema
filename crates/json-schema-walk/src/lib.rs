#![doc = "Shared JSON Schema child traversal utilities."]

use serde_json::Value;
use std::convert::Infallible;

/// Visit direct child schema positions under `schema`.
///
/// The walker follows JSON Schema keywords whose values are themselves schemas,
/// maps of schemas, or arrays of schemas. `$ref` nodes are treated as leaves so
/// callers do not accidentally rewrite inside unresolved reference objects.
pub fn visit_subschemas(schema: &Value, visitor: &mut impl FnMut(&Value)) {
    let Value::Object(object) = schema else {
        return;
    };
    if object.contains_key("$ref") {
        return;
    }

    for key in DIRECT_SCHEMA_KEYS {
        if let Some(value) = object.get(*key) {
            visit_schema_or_schema_array(value, visitor);
        }
    }

    for key in MAP_OF_SCHEMAS_KEYS {
        if let Some(Value::Object(values)) = object.get(*key) {
            for value in values.values() {
                visit_schema_value(value, visitor);
            }
        }
    }

    for key in ARRAY_OF_SCHEMAS_KEYS {
        if let Some(Value::Array(values)) = object.get(*key) {
            for value in values {
                visit_schema_value(value, visitor);
            }
        }
    }

    if let Some(Value::Object(values)) = object.get("dependencies") {
        for value in values.values() {
            visit_schema_value(value, visitor);
        }
    }
}

/// Mutably visit direct child schema positions under `schema`.
pub fn visit_subschemas_mut(schema: &mut Value, visitor: &mut impl FnMut(&mut Value)) {
    let result = try_visit_subschemas_mut(schema, &mut |subschema| {
        visitor(subschema);
        Ok::<(), Infallible>(())
    });
    match result {
        Ok(()) => {}
        Err(err) => match err {},
    }
}

/// Mutably visit direct child schema positions under `schema`, with errors.
///
/// This has the same traversal semantics as [`visit_subschemas_mut`], but lets
/// the visitor fail so callers can propagate their own error type.
pub fn try_visit_subschemas_mut<E>(
    schema: &mut Value,
    visitor: &mut impl FnMut(&mut Value) -> Result<(), E>,
) -> Result<(), E> {
    let Value::Object(object) = schema else {
        return Ok(());
    };
    if object.contains_key("$ref") {
        return Ok(());
    }

    for key in DIRECT_SCHEMA_KEYS {
        if let Some(value) = object.get_mut(*key) {
            visit_schema_or_schema_array_mut(value, visitor)?;
        }
    }

    for key in MAP_OF_SCHEMAS_KEYS {
        if let Some(Value::Object(values)) = object.get_mut(*key) {
            for value in values.values_mut() {
                visit_schema_value_mut(value, visitor)?;
            }
        }
    }

    for key in ARRAY_OF_SCHEMAS_KEYS {
        if let Some(Value::Array(values)) = object.get_mut(*key) {
            for value in values {
                visit_schema_value_mut(value, visitor)?;
            }
        }
    }

    if let Some(Value::Object(values)) = object.get_mut("dependencies") {
        for value in values.values_mut() {
            visit_schema_value_mut(value, visitor)?;
        }
    }

    Ok(())
}

/// Whether a JSON value can syntactically be a JSON Schema at a schema position.
#[must_use]
pub fn is_schema_position(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Bool(_))
}

fn visit_schema_or_schema_array(value: &Value, visitor: &mut impl FnMut(&Value)) {
    match value {
        Value::Array(values) => {
            for value in values {
                visit_schema_value(value, visitor);
            }
        }
        _ => visit_schema_value(value, visitor),
    }
}

fn visit_schema_or_schema_array_mut<E>(
    value: &mut Value,
    visitor: &mut impl FnMut(&mut Value) -> Result<(), E>,
) -> Result<(), E> {
    match value {
        Value::Array(values) => {
            for value in values {
                visit_schema_value_mut(value, visitor)?;
            }
            Ok(())
        }
        _ => visit_schema_value_mut(value, visitor),
    }
}

fn visit_schema_value(value: &Value, visitor: &mut impl FnMut(&Value)) {
    if is_schema_position(value) {
        visitor(value);
    }
}

fn visit_schema_value_mut<E>(
    value: &mut Value,
    visitor: &mut impl FnMut(&mut Value) -> Result<(), E>,
) -> Result<(), E> {
    if is_schema_position(value) {
        visitor(value)?;
    }
    Ok(())
}

const DIRECT_SCHEMA_KEYS: &[&str] = &[
    "additionalItems",
    "additionalProperties",
    "contains",
    "contentSchema",
    "else",
    "if",
    "items",
    "not",
    "propertyNames",
    "then",
    "unevaluatedItems",
    "unevaluatedProperties",
];

const MAP_OF_SCHEMAS_KEYS: &[&str] = &[
    "$defs",
    "definitions",
    "dependentSchemas",
    "patternProperties",
    "properties",
];

const ARRAY_OF_SCHEMAS_KEYS: &[&str] = &["allOf", "anyOf", "oneOf", "prefixItems"];
