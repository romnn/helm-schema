use super::{
    HELM_TRUTHY_DEFINITION_NAME, PLAIN_SCALAR_BOOL_TOKEN_PATTERN,
    PLAIN_SCALAR_DECIMAL_NUMBER_TOKEN_PATTERN, PLAIN_SCALAR_INTEGER_SLOT_TOKEN_PATTERN,
    PLAIN_SCALAR_NULL_TOKEN_PATTERN, PLAIN_SCALAR_NUMBER_TOKEN_PATTERN,
    PLAIN_SCALAR_SPECIAL_FLOAT_TOKEN_PATTERN, SchemaNode, Value, is_annotation_keyword,
    merge_schema_list, schema_type, union_schema_list,
};
use crate::schema_node::JsonSchemaType;

/// Preimage of a provider slot observed through ONE separator-delimited
/// segment of the raw string (tempo's `regexSplit ":" . -1 | last` port
/// suffix): an integer-typed slot admits exactly the strings whose named
/// segment spells an integer. Any other slot type abstains — a string
/// segment leaves the source effectively unconstrained.
pub(super) fn split_segment_provider_preimage(
    schema: &Value,
    segment: &helm_schema_core::SplitSegmentUse,
) -> Option<Value> {
    let pattern = split_segment_pattern(schema, segment)?;
    Some(serde_json::json!({ "type": "string", "pattern": pattern }))
}

/// The accepted-source pattern for a slot observed through one separator
/// segment: only integer-typed slots have a composable segment grammar; any
/// other slot type abstains.
pub(crate) fn split_segment_pattern(
    schema: &Value,
    segment: &helm_schema_core::SplitSegmentUse,
) -> Option<String> {
    if schema_type(schema) != Some("integer") {
        return None;
    }
    let separator = helm_schema_core::escape_regex_literal(&segment.separator);
    Some(if segment.last {
        format!("^([\\s\\S]*{separator})?[+-]?[0-9]+$")
    } else {
        format!("^[+-]?[0-9]+({separator}[\\s\\S]*)?$")
    })
}

#[derive(Default)]
struct ScalarPreimage {
    projected: Value,
    numeric_string_unknown: bool,
}

pub(super) fn plain_scalar_provider_preimage(
    schema: Value,
    placement: helm_schema_core::PlainScalarPlacement,
) -> Value {
    let preimage = plain_scalar_provider_preimage_with(schema, None);
    if preimage.numeric_string_unknown {
        // Draft 7 cannot constrain a parsed string's numeric value without losing valid spellings.
        // The outer union preserves non-string projections and oneOf cardinality.
        tracing::debug!("Bounded numeric provider preimage retains an unknown string domain");
        return serde_json::json!({"anyOf": [preimage.projected, {"type": "string"}]});
    }
    // A newline can rewrite later keys or fold inside a flow scalar; source text is not the final value.
    let newline_escape = match placement {
        helm_schema_core::PlainScalarPlacement::Unknown => {
            serde_json::json!({ "type": "string", "pattern": "[\\r\\n]" })
        }
        helm_schema_core::PlainScalarPlacement::BlockValue => {
            // At a proved block value, the unindented second word cannot continue the first line.
            serde_json::json!({
                "type": "string",
                "pattern": "[\\r\\n]",
                "not": { "pattern": "^[A-Za-z]+\\n[A-Za-z]+$" },
            })
        }
    };
    union_schema_list(vec![preimage.projected, newline_escape])
}

pub(super) fn stringified_plain_scalar_provider_preimage(
    schema: Value,
    placement: helm_schema_core::PlainScalarPlacement,
) -> Value {
    // Raw strings may reparse as structured YAML; its members cannot type the source string.
    if super::go_format::may_allow_type(&schema, "array")
        || super::go_format::may_allow_type(&schema, "object")
    {
        return super::empty_schema();
    }
    let preserves_plain_string = schema_covers_strict_plain_scalar_string(&schema);
    let scalar_preimage = plain_scalar_provider_preimage(schema, placement);
    if preserves_plain_string {
        union_schema_list(vec![
            scalar_preimage,
            printf_string_formattable_mapping_schema(),
        ])
    } else {
        scalar_preimage
    }
}

fn plain_scalar_provider_preimage_with(
    schema: Value,
    parent_type: Option<JsonSchemaType>,
) -> ScalarPreimage {
    let Some(object) = schema.as_object() else {
        return ScalarPreimage {
            projected: schema,
            ..Default::default()
        };
    };
    let effective_type = schema_type(&schema)
        .and_then(JsonSchemaType::from_name)
        .or(parent_type);
    if let Some(types) = object.get("type").and_then(Value::as_array) {
        let mut variants = Vec::new();
        let mut numeric_string_unknown = false;
        for schema_type in types.iter().filter_map(Value::as_str) {
            let mut variant = object.clone();
            variant.insert("type".to_string(), Value::String(schema_type.to_string()));
            let preimage =
                plain_scalar_provider_preimage_with(Value::Object(variant), effective_type);
            numeric_string_unknown |= preimage.numeric_string_unknown;
            variants.push(preimage.projected);
        }
        return ScalarPreimage {
            projected: union_schema_list(variants),
            numeric_string_unknown,
        };
    }
    if matches!(
        effective_type,
        Some(JsonSchemaType::Integer | JsonSchemaType::Number)
    ) && [
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
    ]
    .iter()
    .any(|keyword| object.contains_key(*keyword))
    {
        return ScalarPreimage {
            projected: schema,
            numeric_string_unknown: true,
        };
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(variants) = object.get(keyword).and_then(Value::as_array) {
            let mut projected = Vec::with_capacity(variants.len());
            let mut numeric_string_unknown = false;
            for variant in variants {
                let preimage = plain_scalar_provider_preimage_with(variant.clone(), effective_type);
                numeric_string_unknown |= preimage.numeric_string_unknown;
                projected.push(preimage.projected);
            }
            let mut transformed = object.clone();
            transformed.insert(keyword.to_string(), Value::Array(projected));
            return ScalarPreimage {
                projected: Value::Object(transformed),
                numeric_string_unknown,
            };
        }
    }

    let projected = match schema_type(&schema) {
        Some("integer") => scalar_number_preimage(schema, true),
        Some("number") => scalar_number_preimage(schema, false),
        Some("boolean") => scalar_boolean_preimage(schema),
        Some("string") => scalar_plain_string_preimage(schema),
        Some("null") => scalar_null_preimage(schema),
        _ => schema,
    };
    ScalarPreimage {
        projected,
        ..Default::default()
    }
}

/// The exclusions that keep a PLAIN (unquoted) YAML token intact. `interior`
/// characters end the token wherever they appear; the leading-indicator rules
/// apply only to text that OPENS the token.
pub(crate) fn plain_scalar_structural_exclusions(token_initial: bool) -> Vec<Value> {
    let mut exclusions = Vec::new();
    if token_initial {
        exclusions.push(serde_json::json!({ "not": { "pattern": "^[!&*#{}\\[\\],|>@`%]" } }));
        exclusions.push(serde_json::json!({ "not": { "pattern": "^[-?:]([ \\t]|$)" } }));
    }
    exclusions.push(serde_json::json!({ "not": { "pattern": ":[ \\t]|:$" } }));
    exclusions.push(serde_json::json!({ "not": { "pattern": "[ \\t]#" } }));
    exclusions.push(serde_json::json!({ "not": { "pattern": "[\\r\\n]" } }));
    exclusions
}

pub(crate) fn plain_scalar_safe_comment_string_schema() -> Value {
    serde_json::json!({
        "type": "string",
        "allOf": [
            {
                "pattern": "^[A-Za-z_][A-Za-z0-9_.+/\\-]*[ \\t]+#"
            },
            {
                "not": {
                    "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)[ \\t]+#"
                }
            },
            {
                "not": {
                    "pattern": "^(null|Null|NULL)[ \\t]+#"
                }
            }
        ]
    })
}

pub(crate) fn printf_string_formattable_string_schema() -> Value {
    serde_json::json!({
        "anyOf": [
            {
                "type": "string",
                "allOf": plain_scalar_structural_exclusions(true),
            },
            plain_scalar_safe_comment_string_schema(),
            {
                "type": "string",
                "pattern": "^&[A-Za-z0-9_-]+$",
            },
        ]
    })
}

pub(crate) fn strict_plain_scalar_string_schema() -> Value {
    let mut exclusions = plain_scalar_structural_exclusions(true);
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_NULL_TOKEN_PATTERN }
    }));
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_BOOL_TOKEN_PATTERN }
    }));
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_DECIMAL_NUMBER_TOKEN_PATTERN }
    }));
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_INTEGER_SLOT_TOKEN_PATTERN }
    }));
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_SPECIAL_FLOAT_TOKEN_PATTERN }
    }));
    serde_json::json!({
        "type": "string",
        "allOf": exclusions,
    })
}

pub(crate) fn schema_covers_strict_plain_scalar_string(schema: &Value) -> bool {
    if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
        return variants
            .iter()
            .any(schema_covers_strict_plain_scalar_string);
    }
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        let covering = variants
            .iter()
            .enumerate()
            .filter_map(|(index, variant)| {
                schema_covers_strict_plain_scalar_string(variant).then_some(index)
            })
            .collect::<Vec<_>>();
        let [covering_index] = covering.as_slice() else {
            return false;
        };
        return variants.iter().enumerate().all(|(index, variant)| {
            index == *covering_index || schema_rejects_strict_plain_scalar_string(variant)
        });
    }

    let Some(object) = schema.as_object() else {
        return false;
    };
    if schema_type(schema) != Some("string")
        || !object
            .keys()
            .all(|key| matches!(key.as_str(), "type" | "allOf") || is_annotation_keyword(key))
    {
        return false;
    }
    let Some(candidate_exclusions) = object.get("allOf").and_then(Value::as_array) else {
        return true;
    };
    let strict = strict_plain_scalar_string_schema();
    let Some(strict_exclusions) = strict.get("allOf").and_then(Value::as_array) else {
        return false;
    };
    candidate_exclusions
        .iter()
        .all(|exclusion| strict_exclusions.contains(exclusion))
}

pub(super) fn schema_rejects_strict_plain_scalar_string(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return false;
    };
    let Some(schema_type) = object.get("type") else {
        return false;
    };
    match schema_type {
        Value::String(schema_type) => schema_type != "string",
        Value::Array(schema_types) => !schema_types
            .iter()
            .any(|schema_type| schema_type.as_str() == Some("string")),
        _ => false,
    }
}

pub(crate) fn printf_string_formattable_mapping_schema() -> Value {
    let interior_safe_string = serde_json::json!({
        "type": "string",
        "allOf": plain_scalar_structural_exclusions(false),
    });
    serde_json::json!({
        "type": "object",
        "propertyNames": interior_safe_string.clone(),
        "additionalProperties": {
            "anyOf": [
                { "type": "boolean" },
                { "type": "integer" },
                { "type": "null" },
                { "type": "number" },
                interior_safe_string,
                { "type": "array", "maxItems": 0 },
                { "type": "object", "maxProperties": 0 },
            ]
        }
    })
}

fn scalar_plain_string_preimage(schema: Value) -> Value {
    let mut exclusions = plain_scalar_structural_exclusions(true);
    let unconstrained_string = schema.as_object().is_some_and(|object| {
        [
            "const",
            "enum",
            "format",
            "maxLength",
            "minLength",
            "pattern",
        ]
        .iter()
        .all(|keyword| !object.contains_key(*keyword))
    });
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_NULL_TOKEN_PATTERN }
    }));
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_BOOL_TOKEN_PATTERN }
    }));
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_DECIMAL_NUMBER_TOKEN_PATTERN }
    }));
    // The integer arm and string exclusion share one token grammar so a
    // resolver integer cannot match both `oneOf` arms or fall between them.
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_INTEGER_SLOT_TOKEN_PATTERN }
    }));
    exclusions.push(serde_json::json!({
        "not": { "pattern": PLAIN_SCALAR_SPECIAL_FLOAT_TOKEN_PATTERN }
    }));
    let lexical_domain = serde_json::json!({
        "type": "string",
        "allOf": exclusions
    });
    let string_preimage = merge_schema_list(vec![schema, lexical_domain]);
    if !unconstrained_string {
        return string_preimage;
    }

    // Go formats a mapping as `map[k:v]` at a raw scalar hole. This bounded
    // structural subset keeps every key and value free of the YAML token
    // breakers that could turn that spelling into another node kind.
    union_schema_list(vec![
        string_preimage,
        plain_scalar_safe_comment_string_schema(),
        printf_string_formattable_mapping_schema(),
    ])
}

pub(super) fn scalar_null_preimage(schema: Value) -> Value {
    let rendered_null_string = serde_json::json!({
        "anyOf": [
            {
                "type": "string",
                "pattern": "^[ \\t]*(#.*)?$"
            },
            {
                "type": "string",
                "pattern": "^(~|null|Null|NULL)([ \\t]+#.*)?$"
            },
            {
                "type": "string",
                "pattern": "^&[A-Za-z0-9_-]+[ \\t]*(#.*)?$"
            },
        ]
    });
    union_schema_list(vec![schema, rendered_null_string])
}

fn scalar_number_preimage(schema: Value, integer: bool) -> Value {
    let Some(object) = schema.as_object() else {
        return schema;
    };
    let string_schema = scalar_string_preimage(
        object,
        if integer {
            PLAIN_SCALAR_INTEGER_SLOT_TOKEN_PATTERN
        } else {
            PLAIN_SCALAR_NUMBER_TOKEN_PATTERN
        },
    );
    union_schema_list(vec![schema, string_schema])
}

pub(super) fn scalar_boolean_preimage(schema: Value) -> Value {
    let Some(object) = schema.as_object() else {
        return schema;
    };
    let string_schema = scalar_string_preimage(object, PLAIN_SCALAR_BOOL_TOKEN_PATTERN);
    union_schema_list(vec![schema, string_schema])
}

pub(super) fn scalar_string_preimage(
    object: &serde_json::Map<String, Value>,
    pattern: &str,
) -> Value {
    let mut schema = serde_json::Map::new();
    schema.insert("type".to_string(), Value::String("string".to_string()));
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        schema.insert(
            "enum".to_string(),
            Value::Array(
                values
                    .iter()
                    .map(Value::to_string)
                    .map(Value::String)
                    .collect(),
            ),
        );
    } else if let Some(value) = object.get("const") {
        schema.insert("const".to_string(), Value::String(value.to_string()));
    } else {
        schema.insert("pattern".to_string(), Value::String(pattern.to_string()));
    }
    Value::Object(schema)
}

pub(super) fn helm_falsy_schema() -> Value {
    SchemaNode::not(SchemaNode::reference(
        helm_schema_json_schema_walk::definition_reference(HELM_TRUTHY_DEFINITION_NAME),
    ))
    .into_value()
}

/// Constrains only serialization that `tpl` cannot change, including mapping keys.
pub(crate) fn templated_yaml_provider_preimage(schema: Value) -> Value {
    serialized_yaml_provider_preimage(schema, true)
}

pub(crate) fn serialized_yaml_provider_preimage(schema: Value, templated: bool) -> Value {
    let name = if templated {
        TEMPLATED_MARKER_FREE_DEFINITION_NAME
    } else {
        "helm-marker-free"
    };
    Value::Object(serde_json::Map::from_iter([(
        "anyOf".to_owned(),
        Value::Array(vec![
            serde_json::json!({"not": {"$ref": helm_schema_json_schema_walk::definition_reference(name)}}),
            schema,
        ]),
    )]))
}

pub(crate) const TEMPLATED_MARKER_FREE_DEFINITION_NAME: &str = "helm-template-marker-free";

pub(crate) const NO_TEMPLATE_ACTIONS_DEFINITION_NAME: &str = "helm-no-template-actions";
pub(crate) const YAML_FLOW_STABLE_DEFINITION_NAME: &str = "helm-yaml-flow-stable";

pub(crate) fn unchanged_serialized_schema(name: &str, templated: bool) -> Value {
    let pattern = if templated {
        "\\{\\{|<no value>"
    } else {
        "<no value>"
    };
    recursive_serialized_strings(
        name,
        serde_json::Map::from_iter([("not".to_owned(), serde_json::json!({"pattern": pattern}))]),
    )
}

fn recursive_serialized_strings(
    name: &str,
    mut constraint: serde_json::Map<String, Value>,
) -> Value {
    let reference = helm_schema_json_schema_walk::definition_reference(name);
    let property_names = Value::Object(constraint.clone());
    constraint.insert("type".to_owned(), serde_json::json!("string"));
    let string = Value::Object(constraint);
    serde_json::json!({"anyOf": [
        {"type": ["null", "boolean", "number"]},
        string,
        {"type": "array", "items": {"$ref": reference}},
        {"type": "object", "propertyNames": property_names, "additionalProperties": {"$ref": reference}}
    ]})
}

pub(crate) fn references_serialized_schema(schema: &Value, name: &str) -> bool {
    if schema
        .get("$ref")
        .and_then(Value::as_str)
        .is_some_and(|reference| {
            reference == helm_schema_json_schema_walk::definition_reference(name)
        })
    {
        return true;
    }
    let mut found = false;
    helm_schema_json_schema_walk::visit_subschemas(
        schema,
        helm_schema_json_schema_walk::ReferenceSiblings::Visit,
        &mut |child| {
            found |= references_serialized_schema(child, name);
        },
    );
    found
}

pub(crate) fn serialized_definitions<'a>(
    schemas: impl IntoIterator<Item = &'a Value>,
) -> std::collections::BTreeMap<String, Value> {
    let mut needed = [false; 4];
    for schema in schemas {
        for (required, name) in needed.iter_mut().zip([
            TEMPLATED_MARKER_FREE_DEFINITION_NAME,
            "helm-marker-free",
            YAML_FLOW_STABLE_DEFINITION_NAME,
            NO_TEMPLATE_ACTIONS_DEFINITION_NAME,
        ]) {
            if !*required {
                *required = references_serialized_schema(schema, name);
            }
        }
        if needed.iter().all(|required| *required) {
            break;
        }
    }
    let mut definitions = std::collections::BTreeMap::new();
    for (required, (name, templated)) in needed.into_iter().zip([
        (TEMPLATED_MARKER_FREE_DEFINITION_NAME, true),
        ("helm-marker-free", false),
        (YAML_FLOW_STABLE_DEFINITION_NAME, false),
        (NO_TEMPLATE_ACTIONS_DEFINITION_NAME, false),
    ]) {
        if required {
            definitions.insert(
                name.to_owned(),
                if name == YAML_FLOW_STABLE_DEFINITION_NAME {
                    recursive_serialized_strings(name, serde_json::Map::from_iter([("not".to_owned(), serde_json::json!({"pattern": helm_schema_core::ValuePattern::YAML_FLOW_UNSTABLE_STRING_PATTERN}))]))
                } else if name == NO_TEMPLATE_ACTIONS_DEFINITION_NAME {
                    recursive_serialized_strings(name, serde_json::Map::from_iter([("not".to_owned(), serde_json::json!({"pattern": "\\{\\{"}))]))
                } else {
                    unchanged_serialized_schema(name, templated)
                },
            );
        }
    }
    definitions
}
