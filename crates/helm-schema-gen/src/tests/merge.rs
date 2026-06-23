use super::merge_two_schemas;
use serde_json::Value;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

#[test]
fn merge_open_string_map_with_fixed_values_object_keeps_map_open() {
    let open_map = json!({
        "type": "object",
        "additionalProperties": { "type": "string" }
    });
    let fixed_values_object = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "cert-manager.io/cluster-issuer": { "type": "string" }
        }
    });

    let merged = merge_two_schemas(open_map, fixed_values_object);

    sim_assert_eq!(
        have: merged
            .pointer("/additionalProperties/type")
            .and_then(|value| value.as_str()),
        want: Some("string"),
    );
    sim_assert_eq!(
        have: merged
            .pointer("/properties/cert-manager.io~1cluster-issuer/type")
            .and_then(|value| value.as_str()),
        want: Some("string"),
    );
}

#[test]
fn merge_nested_open_quantity_map_with_fixed_values_object_keeps_map_open() {
    let provider = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "requests": {
                "type": "object",
                "description": "Requests describes the minimum amount of compute resources required.",
                "additionalProperties": {
                    "oneOf": [
                        { "type": "string" },
                        { "type": "number" }
                    ]
                }
            }
        }
    });
    let values_yaml = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "requests": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "cpu": { "type": "string" }
                }
            }
        }
    });

    let merged = merge_two_schemas(provider, values_yaml);

    assert!(
        merged
            .pointer("/properties/requests/additionalProperties/oneOf")
            .and_then(|value| value.as_array())
            .is_some(),
        "expected nested requests map to stay open, got {merged}",
    );
    sim_assert_eq!(
        have: merged
            .pointer("/properties/requests/properties/cpu/type")
            .and_then(|value| value.as_str()),
        want: Some("string"),
    );
}

#[test]
fn merge_preserve_unknown_fields_object_with_closed_values_object_stays_open() {
    let provider = json!({
        "type": "object",
        "x-kubernetes-preserve-unknown-fields": true
    });
    let values_yaml = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "replicas": { "type": "integer" },
            "logLevel": { "type": "string" }
        }
    });

    let merged = merge_two_schemas(provider, values_yaml);

    sim_assert_eq!(
        have: merged.get("additionalProperties"),
        want: None,
        "preserve-unknown-fields object should stay open, got {merged}"
    );
    sim_assert_eq!(
        have: merged
            .pointer("/properties/replicas/type")
            .and_then(serde_json::Value::as_str),
        want: Some("integer"),
    );
    sim_assert_eq!(
        have: merged
            .get("x-kubernetes-preserve-unknown-fields")
            .and_then(serde_json::Value::as_bool),
        want: Some(true),
    );
}

#[test]
fn merge_object_with_conditional_all_of_preserves_branch_constraint() {
    let base = json!({
        "type": "object",
        "additionalProperties": {},
        "properties": {}
    });
    let conditional = json!({
        "type": "object",
        "allOf": [
            {
                "if": {
                    "properties": {
                        "create": { "const": true }
                    },
                    "required": ["create"],
                    "type": "object"
                },
                "then": {
                    "properties": {
                        "annotations": {
                            "type": "object",
                            "additionalProperties": { "type": "string" }
                        }
                    },
                    "type": "object"
                }
            }
        ]
    });

    let merged = merge_two_schemas(base, conditional);

    sim_assert_eq!(
        have: merged
            .pointer("/allOf/0/then/properties/annotations/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "conditional allOf constraints must survive object merges: {merged}"
    );
}

#[test]
fn merge_open_values_object_with_exact_empty_union_preserves_empty_branch() {
    let values_placeholder = json!({
        "type": "object",
        "additionalProperties": {}
    });
    let exact_empty = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {},
        "maxProperties": 0
    });
    let provider = json!({
        "type": "object",
        "required": ["kind", "name"],
        "properties": {
            "kind": { "type": "string" },
            "name": { "type": "string" }
        }
    });

    let merged = merge_two_schemas(
        values_placeholder,
        json!({
            "anyOf": [
                exact_empty,
                provider
            ]
        }),
    );
    let variants = merged
        .get("anyOf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected exact-empty/provider union, got {merged}"));

    assert!(
        variants
            .iter()
            .any(|variant| variant.get("maxProperties").and_then(Value::as_u64) == Some(0)),
        "exact empty object branch should survive merge, got {merged}",
    );
    assert!(
        variants.iter().any(|variant| {
            variant
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| {
                    required.iter().any(|value| value.as_str() == Some("kind"))
                        && required.iter().any(|value| value.as_str() == Some("name"))
                })
        }),
        "provider-required object branch should survive merge, got {merged}",
    );
}

#[test]
fn merge_scalar_schemas_drops_conflicting_annotations() {
    let metadata_name = json!({
        "type": "string",
        "description": "Name must be unique within a namespace."
    });
    let service_account_name = json!({
        "type": "string",
        "description": "ServiceAccountName is the name of the ServiceAccount to use."
    });

    let merged = merge_two_schemas(metadata_name, service_account_name);

    sim_assert_eq!(have: merged, want: json!({ "type": "string" }));
}

#[test]
fn merge_string_schemas_drops_conflicting_validation_keywords() {
    let service_name = json!({
        "type": "string",
        "minLength": 1
    });
    let plain_string = json!({
        "type": "string"
    });

    let merged = merge_two_schemas(service_name, plain_string);
    sim_assert_eq!(have: merged, want: json!({ "type": "string", "minLength": 1 }));

    let merged_nullable = merge_two_schemas(
        json!({ "type": "string", "format": "byte" }),
        json!({ "type": "string", "minLength": 1 }),
    );
    sim_assert_eq!(
        have: merged_nullable,
        want: json!({ "type": "string", "format": "byte", "minLength": 1 })
    );
}

#[test]
fn merge_union_with_contained_variant_is_idempotent() {
    let int_or_string = json!({
        "oneOf": [
            { "type": "string" },
            { "type": "integer" }
        ]
    });

    sim_assert_eq!(
        have: merge_two_schemas(int_or_string.clone(), json!({ "type": "integer" })),
        want: int_or_string
    );
}

#[test]
fn merge_scalar_schema_with_null_keeps_nullable_scalar_schema_compact() {
    let merged = merge_two_schemas(
        json!({
            "description": "priority",
            "format": "int32",
            "type": "integer"
        }),
        json!({ "type": "null" }),
    );

    sim_assert_eq!(
        have: merged,
        want: json!({
            "description": "priority",
            "format": "int32",
            "type": ["integer", "null"]
        })
    );
}

#[test]
fn union_keeps_empty_string_branch_separate_from_non_empty_string() {
    let merged = super::union_schema_list(vec![
        json!({
            "type": "string",
            "minLength": 1
        }),
        json!({
            "type": "string",
            "enum": [""]
        }),
    ]);

    let variants = merged
        .get("anyOf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected non-empty/empty string union, got {merged}"));
    assert!(
        variants
            .iter()
            .any(|variant| variant.get("minLength").and_then(Value::as_u64) == Some(1)),
        "non-empty string branch should survive, got {merged}",
    );
    assert!(
        variants.iter().any(|variant| {
            variant
                .get("enum")
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some("")))
        }),
        "empty string branch should survive, got {merged}",
    );
}

#[test]
fn union_keeps_null_enum_branch_separate_from_string_branch() {
    let merged = super::union_schema_list(vec![
        json!({
            "type": "string"
        }),
        json!({
            "type": "string",
            "enum": [null]
        }),
    ]);

    let variants = merged
        .get("anyOf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected impossible-null/string union, got {merged}"));
    assert!(
        variants
            .iter()
            .any(|variant| variant == &json!({ "type": "string" })),
        "plain string branch should survive, got {merged}",
    );
    assert!(
        variants.iter().any(|variant| {
            variant
                .get("enum")
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().any(Value::is_null))
        }),
        "null enum branch should survive separately, got {merged}",
    );
}
