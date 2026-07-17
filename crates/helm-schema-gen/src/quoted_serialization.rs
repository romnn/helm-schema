//! Shared `$defs` for values rendered inside manually quoted YAML scalars.
//!
//! A raw splice inside manual quotes renders through Go's default
//! formatting: a string embeds verbatim, and a collection serializes as
//! `map[key:value]` / `[item item]` with every nested string and mapping key
//! embedded raw. The quoted token therefore stays intact exactly when every
//! string the value contributes is valid content for the quoting style. Each
//! style encodes that once as a self-referential definition: non-string
//! scalars (formatted as plain digits/words) are always safe, strings and
//! mapping keys must match the style's content grammar, and collections
//! recurse.

use helm_schema_core::QuotedScalarStyle;
use serde_json::{Value, json};

pub(crate) fn definition_name(style: QuotedScalarStyle) -> &'static str {
    match style {
        QuotedScalarStyle::Double => "helm-double-quoted-safe",
        QuotedScalarStyle::Single => "helm-single-quoted-safe",
    }
}

pub(crate) fn reference_schema(style: QuotedScalarStyle) -> Value {
    json!({ "$ref": format!("#/$defs/{}", definition_name(style)) })
}

pub(crate) fn definition_schema(style: QuotedScalarStyle) -> Value {
    let pattern = crate::path_resolver::ecma_compatible_pattern(style.safe_content_pattern())
        .unwrap_or_else(|| style.safe_content_pattern().to_string());
    let reference = reference_schema(style);
    json!({
        "anyOf": [
            { "type": ["boolean", "integer", "null", "number"] },
            { "type": "string", "pattern": pattern },
            { "type": "array", "items": reference },
            {
                "type": "object",
                "propertyNames": { "pattern": pattern },
                "additionalProperties": reference,
            },
        ]
    })
}

pub(crate) fn value_references(value: &Value, style: QuotedScalarStyle) -> bool {
    let needle = format!("#/$defs/{}", definition_name(style));
    references_pointer(value, &needle)
}

fn references_pointer(value: &Value, needle: &str) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, child)| {
            (key == "$ref" && child.as_str() == Some(needle)) || references_pointer(child, needle)
        }),
        Value::Array(items) => items.iter().any(|item| references_pointer(item, needle)),
        _ => false,
    }
}
