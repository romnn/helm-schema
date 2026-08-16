/// Preimage of a provider slot observed through ONE separator-delimited
/// segment of the raw string (tempo's `regexSplit ":" . -1 | last` port
/// suffix): an integer-typed slot admits exactly the strings whose named
/// segment spells an integer. Any other slot type abstains — a string
/// segment leaves the source effectively unconstrained.
fn split_segment_provider_preimage(
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

fn plain_scalar_provider_preimage(schema: Value) -> Value {
    plain_scalar_provider_preimage_with(schema)
}

fn stringified_plain_scalar_provider_preimage(schema: Value) -> Value {
    let preserves_plain_string = schema_covers_strict_plain_scalar_string(&schema);
    let scalar_preimage = plain_scalar_provider_preimage(schema);
    if preserves_plain_string {
        union_schema_list(vec![
            scalar_preimage,
            printf_string_formattable_mapping_schema(),
        ])
    } else {
        scalar_preimage
    }
}

fn plain_scalar_provider_preimage_with(schema: Value) -> Value {
    let Some(object) = schema.as_object() else {
        return schema;
    };
    if let Some(types) = object.get("type").and_then(Value::as_array) {
        let variants = types
            .iter()
            .filter_map(Value::as_str)
            .map(|schema_type| {
                let mut variant = object.clone();
                variant.insert("type".to_string(), Value::String(schema_type.to_string()));
                plain_scalar_provider_preimage_with(Value::Object(variant))
            })
            .collect();
        return union_schema_list(variants);
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(variants) = object.get(keyword).and_then(Value::as_array) {
            let mut transformed = object.clone();
            transformed.insert(
                keyword.to_string(),
                Value::Array(
                    variants
                        .iter()
                        .cloned()
                        .map(plain_scalar_provider_preimage_with)
                        .collect(),
                ),
            );
            return Value::Object(transformed);
        }
    }

    match schema_type(&schema) {
        Some("integer") => scalar_number_preimage(schema, true),
        Some("number") => scalar_number_preimage(schema, false),
        Some("boolean") => scalar_boolean_preimage(schema),
        Some("string") => scalar_plain_string_preimage(schema),
        Some("null") => scalar_null_preimage(schema),
        _ => schema,
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

fn schema_rejects_strict_plain_scalar_string(schema: &Value) -> bool {
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

fn scalar_null_preimage(schema: Value) -> Value {
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
    if [
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
    ]
    .iter()
    .any(|keyword| object.contains_key(*keyword))
    {
        return schema;
    }
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

fn scalar_boolean_preimage(schema: Value) -> Value {
    let Some(object) = schema.as_object() else {
        return schema;
    };
    let string_schema = scalar_string_preimage(object, PLAIN_SCALAR_BOOL_TOKEN_PATTERN);
    union_schema_list(vec![schema, string_schema])
}

fn scalar_string_preimage(object: &serde_json::Map<String, Value>, pattern: &str) -> Value {
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

fn helm_falsy_schema() -> Value {
    SchemaNode::not(SchemaNode::reference(format!(
        "#/$defs/{HELM_TRUTHY_DEFINITION_NAME}"
    )))
    .into_value()
}

/// The branch schema is the strongest available evidence schema that is not a
/// vacuous placeholder when real content exists and accepts the chart's
/// shipped default whenever the branch tolerates its own absence.
/// Projects provider constraints through `tpl (toYaml …)`.
///
/// YAML serialization preserves mappings, sequences, and scalar kinds.
/// `tpl` then changes only strings containing template actions, so those
/// program strings bypass constraints on the rendered string while every
/// structural and action-free constraint remains intact.
fn templated_yaml_provider_preimage(mut schema: Value) -> Value {
    fn admit_template_programs(schema: &mut Value) {
        helm_schema_json_schema_walk::visit_subschemas_mut(schema, &mut admit_template_programs);
        if schema_allows_type(schema, "string") {
            *schema = union_schema_list(vec![
                std::mem::take(schema),
                serde_json::json!({ "type": "string", "pattern": "\\{\\{" }),
            ]);
        }
    }

    admit_template_programs(&mut schema);
    schema
}
