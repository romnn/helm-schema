use super::*;

#[test]
fn declared_scalar_default_survives_active_conjunctive_branch() {
    let unsafe_plain_scalar = serde_json::json!({
        "allOf": [
            {
                "not": {
                    "pattern": "^(|~|null|Null|NULL)$"
                }
            },
            {
                "not": {
                    "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"
                }
            }
        ],
        "type": "string"
    });
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "mode": { "type": "string" },
            "locations": { "type": "array" }
        },
        "allOf": [{
            "if": {
                "properties": { "mode": { "const": "enabled" } },
                "required": ["mode"]
            },
            "then": {
                "properties": {
                    "locations": {
                        "items": {
                            "properties": {
                                "provider": unsafe_plain_scalar
                            },
                            "type": "object"
                        },
                        "type": "array"
                    }
                }
            }
        }]
    });
    let declared = serde_json::json!({
        "mode": "enabled",
        "locations": [{ "provider": "" }]
    });
    let schema = preserve_declared_default_in_schema(schema, &declared);

    assert!(
        schema_accepts_instance(&schema, &declared),
        "the exact chart-authored empty default should survive: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "mode": "enabled",
                "locations": [{ "provider": "true" }]
            })
        ),
        "preserving the exact default must not widen the lexical domain: {schema}"
    );
}

/// A `range`d map whose members are validated by a shared member schema
/// (`additionalProperties`/`items`) still preserves each member's declared
/// empty scalar default, including through the `anyOf` array | object | null
/// member projection and a nullable-sink `anyOf` wrapper on the leaf.
#[test]
fn declared_empty_default_survives_ranged_map_member_projection() {
    let nullable_unsafe_plain_scalar = serde_json::json!({
        "anyOf": [
            {
                "allOf": [
                    {
                        "not": {
                            "pattern": "^(|~|null|Null|NULL)$"
                        }
                    },
                    {
                        "not": {
                            "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"
                        }
                    }
                ],
                "type": "string"
            },
            { "type": "null" }
        ]
    });
    let member = serde_json::json!({
        "type": "object",
        "properties": { "secretName": nullable_unsafe_plain_scalar }
    });
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "databases": {
                "anyOf": [
                    { "type": "object", "additionalProperties": member },
                    { "type": "array", "items": member },
                    { "type": "null" }
                ]
            }
        }
    });
    let declared = serde_json::json!({
        "databases": { "airtype": { "secretName": "" } }
    });
    let schema = preserve_declared_default_in_schema(schema, &declared);

    assert!(
        schema_accepts_instance(&schema, &declared),
        "the declared empty member default must survive the member projection: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "databases": { "airtype": { "secretName": "true" } } })
        ),
        "preserving the exact empty default must not widen the lexical domain: {schema}"
    );
}

#[test]
fn declared_default_does_not_weaken_terminal_false_branch() {
    let schema = serde_json::json!({
        "type": "object",
        "allOf": [false]
    });
    let declared = serde_json::json!({ "enabled": true });
    let schema = preserve_declared_default_in_schema(schema, &declared);

    assert!(
        !schema_accepts_instance(&schema, &declared),
        "a terminal validator must not be bypassed by default preservation: {schema}"
    );
}
