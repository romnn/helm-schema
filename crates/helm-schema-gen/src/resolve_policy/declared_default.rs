use super::{
    HELM_TRUTHY_DEFINITION_NAME, PLAIN_SCALAR_NULL_TOKEN_PATTERN, SchemaNode, Value,
    ValuePathSchemaFacts, empty_schema, helm_truthy_definition_schema, is_object_or_array_schema,
    is_scalar_like_schema, union_schema_list, value_references_helm_truthy,
};

pub(crate) fn open_objects_rejecting_declared_members(schema: Value, declared: &Value) -> Value {
    preserve_declared_default(schema, declared, false)
}

pub(crate) fn preserve_declared_default_in_schema(schema: Value, declared: &Value) -> Value {
    let schema = preserve_declared_default(schema, declared, true);
    preserve_declared_plain_scalar_empty_defaults(schema, declared)
}

pub(super) fn preserve_declared_default(
    mut schema: Value,
    declared: &Value,
    preserve_scalar: bool,
) -> Value {
    let (Some(schema_object), Some(declared_object)) =
        (schema.as_object_mut(), declared.as_object())
    else {
        if let (Some(schema_object), Some(declared_items)) =
            (schema.as_object_mut(), declared.as_array())
            && let Some(items_schema) = schema_object.get_mut("items")
        {
            for declared_item in declared_items {
                *items_schema = preserve_declared_default(
                    std::mem::take(items_schema),
                    declared_item,
                    preserve_scalar,
                );
            }
        }
        return if !preserve_scalar || schema_accepts_json_value(&schema, declared) {
            schema
        } else {
            union_schema_list(vec![
                schema,
                SchemaNode::const_value(declared.clone()).into_value(),
            ])
        };
    };

    for keyword in ["allOf", "anyOf", "oneOf"] {
        let Some(branches) = schema_object.get_mut(keyword).and_then(Value::as_array_mut) else {
            continue;
        };
        for branch in branches {
            *branch = preserve_declared_default(std::mem::take(branch), declared, false);
        }
    }
    for keyword in ["then", "else"] {
        let Some(branch) = schema_object.get_mut(keyword) else {
            continue;
        };
        *branch = preserve_declared_default(std::mem::take(branch), declared, false);
    }

    let known_properties = schema_object
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    // A declared key the schema does not know must stay accepted — the chart
    // ships it — but only that key: dropping the closure outright would also
    // admit every key the sink genuinely rejects. Each unknown declared key
    // becomes an accepted, untyped member instead.
    if schema_object.get("additionalProperties") == Some(&Value::Bool(false)) {
        let unknown: Vec<String> = declared_object
            .keys()
            .filter(|key| !known_properties.contains(*key))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            let properties = schema_object
                .entry("properties")
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if let Some(properties) = properties.as_object_mut() {
                for key in unknown {
                    properties.insert(key, empty_schema());
                }
            } else {
                schema_object.remove("additionalProperties");
            }
        }
    }

    let Some(properties) = schema_object
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    else {
        return schema;
    };
    for (key, child_schema) in properties {
        let Some(child_default) = declared_object.get(key) else {
            continue;
        };
        *child_schema =
            preserve_declared_default(std::mem::take(child_schema), child_default, preserve_scalar);
    }
    schema
}

pub(super) fn preserve_declared_plain_scalar_empty_defaults(
    mut schema: Value,
    declared: &Value,
) -> Value {
    if declared.as_str() == Some("") {
        return if has_plain_scalar_implicit_token_exclusion(&schema)
            && !schema_accepts_json_value(&schema, declared)
        {
            union_schema_list(vec![
                schema,
                SchemaNode::const_value(declared.clone()).into_value(),
            ])
        } else {
            schema
        };
    }

    let Some(schema_object) = schema.as_object_mut() else {
        return schema;
    };
    if let Some(declared_items) = declared.as_array() {
        if let Some(items_schema) = schema_object.get_mut("items") {
            for declared_item in declared_items {
                *items_schema = preserve_declared_plain_scalar_empty_defaults(
                    std::mem::take(items_schema),
                    declared_item,
                );
            }
        }
        return schema;
    }
    let Some(declared_object) = declared.as_object() else {
        return schema;
    };

    // Structural wrappers (`allOf` narrowing branches, `if`/`then`/`else`
    // dispatch, and the `anyOf`/`oneOf` member-projection arms that model a
    // ranged source as array | object | null) carry the same declared
    // default into each branch.
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema_object.get_mut(keyword).and_then(Value::as_array_mut) {
            for branch in branches {
                *branch =
                    preserve_declared_plain_scalar_empty_defaults(std::mem::take(branch), declared);
            }
        }
    }
    for keyword in ["then", "else"] {
        if let Some(branch) = schema_object.get_mut(keyword) {
            *branch =
                preserve_declared_plain_scalar_empty_defaults(std::mem::take(branch), declared);
        }
    }
    if let Some(properties) = schema_object
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    {
        for (key, child_schema) in properties {
            let Some(child_default) = declared_object.get(key) else {
                continue;
            };
            *child_schema = preserve_declared_plain_scalar_empty_defaults(
                std::mem::take(child_schema),
                child_default,
            );
        }
    }
    // A map default whose members are validated by a shared member schema
    // (`range`d over `additionalProperties`/`items`) preserves each declared
    // member's empty scalar defaults through that one member schema.
    for keyword in ["additionalProperties", "items"] {
        if let Some(member_schema) = schema_object.get_mut(keyword)
            && member_schema.is_object()
        {
            for declared_value in declared_object.values() {
                *member_schema = preserve_declared_plain_scalar_empty_defaults(
                    std::mem::take(member_schema),
                    declared_value,
                );
            }
        }
    }
    schema
}

pub(super) fn has_plain_scalar_implicit_token_exclusion(schema: &Value) -> bool {
    SchemaNode::from_value(schema.clone()).has_negated_pattern(PLAIN_SCALAR_NULL_TOKEN_PATTERN)
}

pub(super) fn should_merge_values_yaml_into_conditional_branch(
    branch_schema: &Value,
    values_yaml_schema: &Value,
) -> bool {
    crate::schema_model::is_empty_schema(branch_schema)
        || (is_scalar_like_schema(branch_schema) && is_scalar_like_schema(values_yaml_schema))
}

pub(super) fn schema_accepts_json_value(schema: &Value, instance: &Value) -> bool {
    let document = value_references_helm_truthy(schema).then(|| {
        serde_json::json!({
            "$defs": {
                HELM_TRUTHY_DEFINITION_NAME: helm_truthy_definition_schema()
            },
            "allOf": [schema]
        })
    });
    jsonschema::validator_for(document.as_ref().unwrap_or(schema))
        .map(|validator| validator.is_valid(instance))
        .unwrap_or(false)
}

pub(super) fn should_open_fragment_values_schema(
    schema: &Value,
    facts: ValuePathSchemaFacts,
) -> bool {
    !facts.values_yaml.is_mapping
        || facts.values_yaml.is_empty_map
        || fixed_object_schema_has_object_or_array_child(schema)
}

pub(super) fn fixed_object_schema_has_object_or_array_child(schema: &Value) -> bool {
    schema
        .as_object()
        .and_then(|object| object.get("properties"))
        .and_then(Value::as_object)
        .is_some_and(|properties| properties.values().any(is_object_or_array_schema))
}

pub(super) fn schema_type_for_guard_value(value: &Value) -> Option<&'static str> {
    match value {
        Value::String(_) => Some("string"),
        Value::Bool(_) => Some("boolean"),
        Value::Number(number) if number.is_i64() || number.is_u64() => Some("integer"),
        Value::Number(_) => Some("number"),
        Value::Null => Some("null"),
        _ => None,
    }
}
