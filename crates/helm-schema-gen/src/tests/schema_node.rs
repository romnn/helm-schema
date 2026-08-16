use std::fs;

use color_eyre::eyre::{self, WrapErr as _};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::schema_node::{SchemaNode, TypedSchemaNode};

#[test]
fn lossless_schema_node_round_trips_required_schema_families() -> eyre::Result<()> {
    let workspace = test_util::workspace_root();
    let provider = read_json(&workspace.join(
        "testdata/provider-bundle/kubernetes-json-schema-cache/default/v1.24.0/\
             horizontalpodautoscaler-autoscaling-v2beta1.json",
    ))?;
    let generated = read_json(
        &workspace.join("crates/helm-schema-gen/tests/fixtures/nats_service.schema.json"),
    )?;
    let schemas = [
        provider,
        generated,
        Value::Bool(true),
        Value::Bool(false),
        json!({
            "type": "object",
            "properties": { "known": { "type": "string" } },
            "x-provider-extension": { "ordered": [3, 2, 1] },
        }),
        json!({
            "allOf": [
                { "if": { "properties": { "mode": { "const": "strict" } } } },
                {
                    "then": { "required": ["value"] },
                    "else": { "anyOf": [false, { "not": { "type": "null" } }] },
                },
            ],
            "oneOf": [
                { "type": ["string", "null"] },
                { "items": { "type": "integer" }, "minItems": 1, "type": "array" },
            ],
            "x-mixed-combinator": true,
        }),
    ];

    for schema in schemas {
        sim_assert_eq!(
            have: SchemaNode::from_value(schema.clone()).into_value(),
            want: schema
        );
    }
    Ok(())
}

#[test]
fn lossless_schema_node_types_known_keywords_and_retains_unknown_keywords() -> eyre::Result<()> {
    let original = json!({
        "additionalProperties": false,
        "maxProperties": 4,
        "minProperties": 1,
        "properties": { "name": { "type": "string" } },
        "required": ["name"],
        "type": "object",
        "x-kubernetes-preserve-unknown-fields": true,
    });
    let schema = SchemaNode::from_value(original.clone());

    if !matches!(schema, SchemaNode::Typed(TypedSchemaNode::Keywords(_))) {
        return Err(eyre::eyre!(
            "object schema did not use the lossless typed carrier"
        ));
    }
    sim_assert_eq!(have: schema.into_value(), want: original);
    Ok(())
}

#[test]
fn schema_node_carries_generator_provenance_in_typed_keywords() -> eyre::Result<()> {
    let truthy_reference = "#/$defs/t";
    let null_pattern = crate::resolve_policy::PLAIN_SCALAR_NULL_TOKEN_PATTERN;
    let schema = SchemaNode::from_value(json!({
        "anyOf": [
            {
                "allOf": [
                    { "not": { "pattern": null_pattern } },
                    { "type": "string" },
                ]
            },
            {
                "properties": {
                    "enabled": { "$ref": truthy_reference }
                }
            }
        ]
    }));

    if !schema.has_negated_pattern(null_pattern) {
        return Err(eyre::eyre!("typed schema lost plain-scalar provenance"));
    }
    if !schema.references(truthy_reference) {
        return Err(eyre::eyre!("typed schema lost Helm-truthy provenance"));
    }
    Ok(())
}

fn read_json(path: &std::path::Path) -> eyre::Result<Value> {
    let source = fs::read_to_string(path)
        .wrap_err_with(|| format!("read schema fixture {}", path.display()))?;
    serde_json::from_str(&source)
        .wrap_err_with(|| format!("parse schema fixture {}", path.display()))
}
