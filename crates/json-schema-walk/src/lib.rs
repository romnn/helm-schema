#![doc = "Shared JSON Schema child traversal utilities."]

use serde_json::Value;
use std::convert::Infallible;

pub fn canonical_json_string(value: &Value) -> String {
    serde_json::to_string(&canonical_json_value(value)).expect("serialize canonical JSON value")
}

fn canonical_json_value(value: &Value) -> Value {
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

/// Escape one JSON Pointer segment per RFC 6901 (`~` → `~0`, `/` → `~1`).
#[must_use]
pub fn escape_json_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaTraversalContext {
    Schema,
    SchemaArray,
    SchemaMapValues,
    /// Value of a `$ref` keyword: a reference target, not a subschema.
    Ref,
    Data,
}

pub fn schema_child_context_for_keyword(key: &str) -> SchemaTraversalContext {
    match key {
        "properties" | "patternProperties" | "$defs" | "definitions" | "dependencies"
        | "dependentSchemas" => SchemaTraversalContext::SchemaMapValues,
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
        "$ref" => SchemaTraversalContext::Ref,
        _ => SchemaTraversalContext::Data,
    }
}

pub fn ref_points_inside(root: &Value, reference: &str) -> bool {
    let Some(pointer) = reference.strip_prefix('#') else {
        return false;
    };
    pointer.is_empty() || root.pointer(pointer).is_some()
}

/// Whether every internal (`#`-prefixed) `$ref` in `schema` resolves within
/// `schema` itself.
pub fn schema_refs_point_inside(schema: &Value) -> bool {
    refs_point_inside_value(schema, schema, SchemaTraversalContext::Schema)
}

fn refs_point_inside_value(root: &Value, value: &Value, context: SchemaTraversalContext) -> bool {
    match value {
        Value::String(reference) if context == SchemaTraversalContext::Ref => {
            ref_points_inside(root, reference)
        }
        Value::Array(values) => match context {
            SchemaTraversalContext::Data | SchemaTraversalContext::Ref => true,
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => values
                .iter()
                .all(|value| refs_point_inside_value(root, value, SchemaTraversalContext::Schema)),
            SchemaTraversalContext::SchemaMapValues => values.iter().all(|value| {
                refs_point_inside_value(root, value, SchemaTraversalContext::SchemaMapValues)
            }),
        },
        Value::Object(object) => match context {
            SchemaTraversalContext::Data | SchemaTraversalContext::Ref => true,
            SchemaTraversalContext::SchemaMapValues => object
                .values()
                .all(|value| refs_point_inside_value(root, value, SchemaTraversalContext::Schema)),
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                object.iter().all(|(key, value)| {
                    refs_point_inside_value(root, value, schema_child_context_for_keyword(key))
                })
            }
        },
        _ => true,
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
            SchemaTraversalContext::Data | SchemaTraversalContext::Ref => {
                Ok(Value::Array(values.clone()))
            }
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
            SchemaTraversalContext::Data | SchemaTraversalContext::Ref => {
                Ok(Value::Object(object.clone()))
            }
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

    for (key, value) in object {
        match schema_child_context_for_keyword(key) {
            SchemaTraversalContext::Schema => match value {
                Value::Array(values) => {
                    for value in values {
                        visit_schema_value(value, visitor);
                    }
                }
                _ => visit_schema_value(value, visitor),
            },
            SchemaTraversalContext::SchemaArray => {
                if let Value::Array(values) = value {
                    for value in values {
                        visit_schema_value(value, visitor);
                    }
                }
            }
            SchemaTraversalContext::SchemaMapValues => {
                if let Value::Object(values) = value {
                    for value in values.values() {
                        visit_schema_value(value, visitor);
                    }
                }
            }
            SchemaTraversalContext::Data | SchemaTraversalContext::Ref => {}
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

    for (key, value) in object {
        match schema_child_context_for_keyword(key) {
            SchemaTraversalContext::Schema => match value {
                Value::Array(values) => {
                    for value in values {
                        visit_schema_value_mut(value, visitor)?;
                    }
                }
                _ => visit_schema_value_mut(value, visitor)?,
            },
            SchemaTraversalContext::SchemaArray => {
                if let Value::Array(values) = value {
                    for value in values {
                        visit_schema_value_mut(value, visitor)?;
                    }
                }
            }
            SchemaTraversalContext::SchemaMapValues => {
                if let Value::Object(values) = value {
                    for value in values.values_mut() {
                        visit_schema_value_mut(value, visitor)?;
                    }
                }
            }
            SchemaTraversalContext::Data | SchemaTraversalContext::Ref => {}
        }
    }

    Ok(())
}

/// Whether a JSON value can syntactically be a JSON Schema at a schema position.
#[must_use]
pub(crate) fn is_schema_position(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Bool(_))
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
