use color_eyre::eyre::{self, OptionExt as _};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::{OwnedDefinitions, prune_unreachable_owned_definitions};

#[test]
fn jsonschema_resolver_decodes_the_whole_uri_fragment_before_pointer_splitting() -> eyre::Result<()>
{
    let encoded_separator = json!({
        "$defs": {
            "a": { "b": { "const": "nested" } },
            "a/b": { "const": "flat" },
        },
        "$ref": "#/$defs/a%2Fb",
    });
    let validator = jsonschema::validator_for(&encoded_separator)
        .map_err(|error| eyre::eyre!("compile encoded-separator schema: {error}"))?;
    sim_assert_eq!(have: validator.is_valid(&json!("nested")), want: true);
    sim_assert_eq!(have: validator.is_valid(&json!("flat")), want: false);

    let encoded_tilde_escape = json!({
        "$defs": {
            "a/b": { "const": "slash" },
            "a~1b": { "const": "literal-tilde" },
        },
        "$ref": "#/$defs/a%7E1b",
    });
    let validator = jsonschema::validator_for(&encoded_tilde_escape)
        .map_err(|error| eyre::eyre!("compile encoded-tilde schema: {error}"))?;
    sim_assert_eq!(have: validator.is_valid(&json!("slash")), want: true);
    sim_assert_eq!(have: validator.is_valid(&json!("literal-tilde")), want: false);

    let encoded_pointer = json!({
        "$defs": {
            "name": { "const": "decoded" },
        },
        "$ref": "#%2F$defs%2Fname",
    });
    let error = jsonschema::validator_for(&encoded_pointer)
        .err()
        .ok_or_eyre("encoded anchor unexpectedly compiled as a JSON Pointer")?;
    sim_assert_eq!(
        have: error.to_string().contains("Anchor '%2F$defs%2Fname' does not exist"),
        want: true
    );
    Ok(())
}

#[test]
fn late_prune_closes_transitive_reachability_and_removes_only_orphans() {
    let mut schema = json!({
        "$defs": {
            "kept": { "$ref": "#/$defs/transitive" },
            "orphan": { "type": "number" },
            "transitive": { "type": "string" },
        },
        "properties": {
            "value": { "$ref": "#/$defs/kept" },
        },
        "type": "object",
    });
    let owned = OwnedDefinitions::capture(&schema).retain_unchanged(&schema);

    let removed = prune_unreachable_owned_definitions(&mut schema, &owned);

    sim_assert_eq!(have: removed, want: 1);
    sim_assert_eq!(
        have: schema.get("$defs"),
        want: Some(&json!({
            "kept": { "$ref": "#/$defs/transitive" },
            "transitive": { "type": "string" },
        }))
    );
}

#[test]
fn late_prune_preserves_caller_modified_and_caller_added_definitions() {
    let generated = json!({
        "$defs": {
            "generated": { "type": "string" },
        },
        "type": "object",
    });
    let captured = OwnedDefinitions::capture(&generated);
    let mut overridden = json!({
        "$defs": {
            "caller": { "type": "boolean" },
            "generated": { "type": "integer" },
        },
        "type": "object",
    });
    let owned = captured.retain_unchanged(&overridden);

    let removed = prune_unreachable_owned_definitions(&mut overridden, &owned);

    sim_assert_eq!(have: removed, want: 0);
    sim_assert_eq!(
        have: overridden.get("$defs"),
        want: Some(&json!({
            "caller": { "type": "boolean" },
            "generated": { "type": "integer" },
        }))
    );
}

#[test]
fn retained_caller_definition_keeps_owned_transitive_references() {
    let generated = json!({
        "$defs": {
            "caller": { "type": "string" },
            "owned": { "type": "integer" },
        },
        "type": "object",
    });
    let captured = OwnedDefinitions::capture(&generated);
    let mut overridden = json!({
        "$defs": {
            "caller": { "$ref": "#/$defs/owned" },
            "owned": { "type": "integer" },
        },
        "type": "object",
    });
    let owned = captured.retain_unchanged(&overridden);

    let removed = prune_unreachable_owned_definitions(&mut overridden, &owned);

    sim_assert_eq!(have: removed, want: 0);
    assert!(overridden.pointer("/$defs/owned").is_some());
}

#[test]
fn late_prune_decodes_definition_names_in_json_pointer_refs() {
    let mut schema = json!({
        "$defs": {
            "provider/name~shape": { "type": "string" },
        },
        "$ref": "#/$defs/provider~1name~0shape",
    });
    let owned = OwnedDefinitions::capture(&schema).retain_unchanged(&schema);

    let removed = prune_unreachable_owned_definitions(&mut schema, &owned);

    sim_assert_eq!(have: removed, want: 0);
}

#[test]
fn late_prune_decodes_percent_encoded_definition_refs() {
    let mut schema = json!({
        "$defs": {
            "plain": { "type": "string" },
            "space name": { "type": "integer" },
        },
        "allOf": [
            { "$ref": "#/%24defs/plain" },
            { "$ref": "#/$defs/space%20name" },
        ],
    });
    let owned = OwnedDefinitions::capture(&schema).retain_unchanged(&schema);

    let removed = prune_unreachable_owned_definitions(&mut schema, &owned);

    sim_assert_eq!(have: removed, want: 0);
}

#[test]
fn late_prune_percent_decodes_before_json_pointer_tilde_decoding() {
    let mut schema = json!({
        "$defs": {
            "a/b": { "type": "string" },
            "a~1b": { "type": "integer" },
        },
        "$ref": "#/$defs/a%7E1b",
    });
    let owned = OwnedDefinitions::capture(&schema).retain_unchanged(&schema);

    let removed = prune_unreachable_owned_definitions(&mut schema, &owned);

    sim_assert_eq!(have: removed, want: 1);
    sim_assert_eq!(
        have: schema.get("$defs"),
        want: Some(&json!({ "a/b": { "type": "string" } }))
    );
}

#[test]
fn late_prune_decodes_the_whole_fragment_before_splitting_pointer_segments() {
    let mut schema = json!({
        "$defs": {
            "provider": {
                "properties": {
                    "name": { "type": "string" },
                },
                "type": "object",
            },
            "provider/name": { "type": "boolean" },
            "orphan": { "type": "integer" },
        },
        "$ref": "#/%24defs/provider%2Fname",
    });
    let owned = OwnedDefinitions::capture(&schema).retain_unchanged(&schema);

    let removed = prune_unreachable_owned_definitions(&mut schema, &owned);

    sim_assert_eq!(have: removed, want: 2);
    sim_assert_eq!(
        have: schema.get("$defs"),
        want: Some(&json!({
            "provider": {
                "properties": {
                    "name": { "type": "string" },
                },
                "type": "object",
            },
        }))
    );
}

#[test]
fn late_prune_preserves_definitions_for_undecodable_local_refs() {
    let mut schema = json!({
        "$defs": {
            "one": { "type": "string" },
            "two": { "type": "integer" },
        },
        "$ref": "#/$defs/%FF",
    });
    let owned = OwnedDefinitions::capture(&schema).retain_unchanged(&schema);

    let removed = prune_unreachable_owned_definitions(&mut schema, &owned);

    sim_assert_eq!(have: removed, want: 0);
}

#[test]
fn late_prune_follows_root_refs_from_nested_definition_scopes() {
    let mut schema = json!({
        "$defs": {
            "rootTarget": { "type": "string" },
        },
        "properties": {
            "value": {
                "$defs": {
                    "local": { "$ref": "#/$defs/rootTarget" },
                },
                "$ref": "#/$defs/local",
            },
        },
        "type": "object",
    });
    let owned = OwnedDefinitions::capture(&schema).retain_unchanged(&schema);

    let removed = prune_unreachable_owned_definitions(&mut schema, &owned);

    sim_assert_eq!(have: removed, want: 0);
    assert!(schema.pointer("/$defs/rootTarget").is_some());
}

#[test]
fn late_prune_is_validation_equivalent_across_definition_keywords() -> eyre::Result<()> {
    let mut after = json!({
        "$defs": {
            "kept": { "type": "string" },
            "orphan": { "type": "boolean" },
        },
        "definitions": {
            "kept": { "type": "integer" },
            "orphan": { "type": "array" },
        },
        "anyOf": [
            { "$ref": "#/$defs/kept" },
            { "$ref": "#/definitions/kept" },
        ],
    });
    let before = after.clone();
    let owned = OwnedDefinitions::capture(&after).retain_unchanged(&after);

    sim_assert_eq!(
        have: prune_unreachable_owned_definitions(&mut after, &owned),
        want: 2
    );
    let before_validator = jsonschema::validator_for(&before)
        .map_err(|error| eyre::eyre!("compile pre-prune schema: {error}"))?;
    let after_validator = jsonschema::validator_for(&after)
        .map_err(|error| eyre::eyre!("compile post-prune schema: {error}"))?;
    for probe in [
        json!(null),
        json!(false),
        json!(0),
        json!(1.5),
        json!("value"),
        json!([]),
        json!({}),
    ] {
        sim_assert_eq!(
            have: after_validator.is_valid(&probe),
            want: before_validator.is_valid(&probe),
            "late reachability pruning changed acceptance for {probe}"
        );
    }
    Ok(())
}
