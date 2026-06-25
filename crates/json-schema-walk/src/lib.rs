#![doc = "Shared JSON Schema child traversal utilities."]

use serde_json::Value;
use std::convert::Infallible;

pub fn canonical_json_string(value: &Value) -> String {
    serde_json::to_string(&canonical_json_value(value)).expect("serialize canonical JSON value")
}

pub fn canonical_json_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut out = serde_json::Map::new();
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            for key in keys {
                if let Some(value) = object.get(key) {
                    out.insert(key.clone(), canonical_json_value(value));
                }
            }
            Value::Object(out)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_json_value).collect()),
        other => other.clone(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaTraversalContext {
    Schema,
    SchemaArray,
    SchemaMapValues,
    Data,
}

pub fn schema_child_context_for_keyword(key: &str) -> SchemaTraversalContext {
    match key {
        "properties" | "patternProperties" | "$defs" | "definitions" | "dependentSchemas" => {
            SchemaTraversalContext::SchemaMapValues
        }
        "allOf" | "anyOf" | "oneOf" | "prefixItems" => SchemaTraversalContext::SchemaArray,
        "additionalItems"
        | "additionalProperties"
        | "contains"
        | "contentSchema"
        | "else"
        | "if"
        | "items"
        | "not"
        | "propertyNames"
        | "then"
        | "unevaluatedItems"
        | "unevaluatedProperties" => SchemaTraversalContext::Schema,
        _ => SchemaTraversalContext::Data,
    }
}

pub fn ref_points_inside(root: &Value, reference: &str) -> bool {
    let Some(pointer) = reference.strip_prefix('#') else {
        return false;
    };
    pointer.is_empty() || root.pointer(pointer).is_some()
}

pub fn schema_refs_point_inside(root: &Value, schema: &Value) -> bool {
    schema_refs_point_inside_value(root, schema, SchemaTraversalContext::Schema)
}

pub fn schema_refs_point_inside_value(
    root: &Value,
    value: &Value,
    context: SchemaTraversalContext,
) -> bool {
    match value {
        Value::Array(values) => match context {
            SchemaTraversalContext::Data => true,
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                values.iter().all(|value| {
                    schema_refs_point_inside_value(root, value, SchemaTraversalContext::Schema)
                })
            }
            SchemaTraversalContext::SchemaMapValues => values.iter().all(|value| {
                schema_refs_point_inside_value(root, value, SchemaTraversalContext::SchemaMapValues)
            }),
        },
        Value::Object(object) => match context {
            SchemaTraversalContext::Data => true,
            SchemaTraversalContext::SchemaMapValues => object.values().all(|value| {
                schema_refs_point_inside_value(root, value, SchemaTraversalContext::Schema)
            }),
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                if let Some(reference) = object.get("$ref").and_then(Value::as_str)
                    && !ref_points_inside(root, reference)
                {
                    return false;
                }
                object.iter().all(|(key, value)| {
                    schema_refs_point_inside_value(
                        root,
                        value,
                        schema_child_context_for_keyword(key),
                    )
                })
            }
        },
        _ => true,
    }
}

pub fn try_rewrite_schema_refs<E>(
    value: &Value,
    context: SchemaTraversalContext,
    mut rewrite_ref: impl FnMut(&Value) -> Result<Value, E>,
) -> Result<Value, E> {
    try_rewrite_schema_refs_in(value, context, &mut rewrite_ref)
}

fn try_rewrite_schema_refs_in<E>(
    value: &Value,
    context: SchemaTraversalContext,
    rewrite_ref: &mut impl FnMut(&Value) -> Result<Value, E>,
) -> Result<Value, E> {
    match value {
        Value::Array(values) => match context {
            SchemaTraversalContext::Data => Ok(Value::Array(values.clone())),
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => values
                .iter()
                .map(|value| {
                    try_rewrite_schema_refs_in(value, SchemaTraversalContext::Schema, rewrite_ref)
                })
                .collect(),
            SchemaTraversalContext::SchemaMapValues => values
                .iter()
                .map(|value| {
                    try_rewrite_schema_refs_in(
                        value,
                        SchemaTraversalContext::SchemaMapValues,
                        rewrite_ref,
                    )
                })
                .collect(),
        },
        Value::Object(object) => match context {
            SchemaTraversalContext::Data => Ok(Value::Object(object.clone())),
            SchemaTraversalContext::SchemaMapValues => object
                .iter()
                .map(|(key, value)| {
                    try_rewrite_schema_refs_in(value, SchemaTraversalContext::Schema, rewrite_ref)
                        .map(|value| (key.clone(), value))
                })
                .collect(),
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => object
                .iter()
                .map(|(key, value)| {
                    let value = if key == "$ref" {
                        rewrite_ref(value)?
                    } else {
                        try_rewrite_schema_refs_in(
                            value,
                            schema_child_context_for_keyword(key),
                            rewrite_ref,
                        )?
                    };
                    Ok((key.clone(), value))
                })
                .collect(),
        },
        _ => Ok(value.clone()),
    }
}

pub fn try_map_schema_context<E>(
    value: &Value,
    context: SchemaTraversalContext,
    mut rewrite: impl FnMut(&Value, SchemaTraversalContext, usize) -> Result<Option<Value>, E>,
) -> Result<Value, E> {
    try_map_schema_context_at(value, context, 0, &mut rewrite)
}

fn try_map_schema_context_at<E>(
    value: &Value,
    context: SchemaTraversalContext,
    depth: usize,
    rewrite: &mut impl FnMut(&Value, SchemaTraversalContext, usize) -> Result<Option<Value>, E>,
) -> Result<Value, E> {
    if let Some(rewritten) = rewrite(value, context, depth)? {
        return Ok(rewritten);
    }

    match value {
        Value::Array(values) => match context {
            SchemaTraversalContext::Data => Ok(Value::Array(values.clone())),
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => values
                .iter()
                .map(|value| {
                    try_map_schema_context_at(
                        value,
                        SchemaTraversalContext::Schema,
                        depth + 1,
                        rewrite,
                    )
                })
                .collect(),
            SchemaTraversalContext::SchemaMapValues => values
                .iter()
                .map(|value| {
                    try_map_schema_context_at(
                        value,
                        SchemaTraversalContext::SchemaMapValues,
                        depth + 1,
                        rewrite,
                    )
                })
                .collect(),
        },
        Value::Object(object) => match context {
            SchemaTraversalContext::Data => Ok(Value::Object(object.clone())),
            SchemaTraversalContext::SchemaMapValues => object
                .iter()
                .map(|(key, value)| {
                    try_map_schema_context_at(
                        value,
                        SchemaTraversalContext::Schema,
                        depth + 1,
                        rewrite,
                    )
                    .map(|value| (key.clone(), value))
                })
                .collect(),
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => object
                .iter()
                .map(|(key, value)| {
                    try_map_schema_context_at(
                        value,
                        schema_child_context_for_keyword(key),
                        depth + 1,
                        rewrite,
                    )
                    .map(|value| (key.clone(), value))
                })
                .collect(),
        },
        _ => Ok(value.clone()),
    }
}

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
