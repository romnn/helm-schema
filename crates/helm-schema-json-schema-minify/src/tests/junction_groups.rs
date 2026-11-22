use std::collections::BTreeMap;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_json_schema_walk::GENERATED_DEFINITIONS;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use super::{flatten_junctions, share_reference_groups};
use crate::{DefinitionNames, minimize_schema, shorten_definition_names};

fn reference(name: &str) -> Value {
    json!({"$ref": GENERATED_DEFINITIONS.reference(name)})
}

fn overlapping_junctions(common: usize, width: usize) -> Value {
    let mut definitions = (0..2 * width - common)
        .map(|index| (format!("v{index}"), json!({"const": index})))
        .collect::<BTreeMap<_, _>>();
    let operands = |range: std::ops::Range<usize>| {
        range
            .map(|index| reference(&format!("v{index}")))
            .collect::<Vec<_>>()
    };
    definitions.insert("left".to_owned(), json!({"anyOf": operands(0..width)}));
    let mut right = operands(0..common);
    right.extend(operands(width..2 * width - common));
    definitions.insert("right".to_owned(), json!({"anyOf": right}));
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": definitions,
        "properties": {"left": reference("left"), "right": reference("right")},
        "type": "object"
    })
}

#[test]
fn flattening_splices_single_use_refs_and_preserves_addressed_arms() -> eyre::Result<()> {
    let mut schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {
            "addressed": {"anyOf": [reference("addressed_child"), {"const": 4}]},
            "addressed_child": {"anyOf": [{"const": 5}, {"const": 6}]},
            "child": {"anyOf": [{"const": 0}, {"const": 1}]}
        },
        "properties": {
            "left": {"anyOf": [reference("child"), {"anyOf": [{"const": 2}, {"const": 3}]}]},
            "arm": {"$ref": "#/definitions/addressed/anyOf/1"}
        },
        "type": "object"
    });
    let mut expected = schema.clone();
    expected["definitions"]
        .as_object_mut()
        .ok_or_eyre("expected definitions")?
        .remove("child");
    expected["properties"]["left"] =
        json!({"anyOf": [{"const": 0}, {"const": 1}, {"const": 2}, {"const": 3}]});
    flatten_junctions(&mut schema);
    sim_assert_eq!(have: &schema, want: &expected);
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&json!({"left": 3, "arm": 4})));
    assert!(!validator.is_valid(&json!({"left": 3, "arm": 5})));
    Ok(())
}

#[test]
fn a_repeated_pair_with_no_byte_saving_stays_inline() {
    let before = overlapping_junctions(2, 8);
    let mut after = before.clone();
    share_reference_groups(&mut after);
    sim_assert_eq!(have: after, want: before);
}

#[test]
fn balancing_preserves_indexes_in_an_addressed_wide_junction() -> eyre::Result<()> {
    let mut schema = overlapping_junctions(24, 32);
    schema["properties"]["addressed"] = json!({"$ref":"#/definitions/left/anyOf/31"});
    schema["properties"]["another"] = reference("another");
    let addressed = schema["definitions"]["left"].clone();
    schema["definitions"]["another"] = addressed.clone();
    share_reference_groups(&mut schema);

    sim_assert_eq!(have: &schema["definitions"]["left"], want: &addressed);
    assert!(has_shared_group(&schema));
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&json!({"addressed":31, "right":39, "another":15})));
    assert!(!validator.is_valid(&json!({"addressed":23, "right":39, "another":15})));
    Ok(())
}

#[test]
fn balanced_groups_save_bytes_and_preserve_both_junctions() -> eyre::Result<()> {
    let before = overlapping_junctions(24, 32);
    let mut after = before.clone();
    share_reference_groups(&mut after);
    assert!(has_shared_group(&after));
    assert!(after.to_string().len() < before.to_string().len());
    assert!(
        shorten_definition_names(&after).schema.to_string().len()
            < shorten_definition_names(&before).schema.to_string().len()
    );
    assert!(reference_depth(&after, &after)? <= 4);

    let original = jsonschema::validator_for(&before)?;
    let balanced = jsonschema::validator_for(&after)?;
    for left in 0..=40 {
        for right in 0..=40 {
            let instance = json!({"left": left, "right": right});
            let accepted = left < 32 && (right < 24 || (32..40).contains(&right));
            sim_assert_eq!(have: original.is_valid(&instance), want: accepted);
            sim_assert_eq!(have: balanced.is_valid(&instance), want: accepted);
        }
    }
    Ok(())
}

#[test]
fn minimized_schema_shares_junctions_after_inlining() -> eyre::Result<()> {
    let schema = overlapping_junctions(24, 32);
    let definitions = BTreeMap::from([("left".to_owned(), Vec::new())]);
    let schema = minimize_schema(schema, &definitions, DefinitionNames::Source);

    assert!(has_shared_group(&schema));
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&json!({"left": 31, "right": 39})));
    assert!(!validator.is_valid(&json!({"left": 32, "right": 39})));
    Ok(())
}

fn has_shared_group(schema: &Value) -> bool {
    schema["definitions"]
        .as_object()
        .is_some_and(|definitions| {
            definitions
                .keys()
                .any(|name| name.starts_with("constrains/group-"))
        })
}

fn reference_depth(schema: &Value, node: &Value) -> eyre::Result<usize> {
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_eyre("expected a local ref")?;
        let target = schema
            .pointer(pointer)
            .ok_or_eyre("missing reference target")?;
        return Ok(1 + reference_depth(schema, target)?);
    }
    let children = node
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(keyword, _)| !std::ptr::eq(node, schema) || *keyword != "definitions")
        .map(|(_, child)| child)
        .chain(node.as_array().into_iter().flatten());
    let mut maximum = 0;
    for child in children {
        maximum = maximum.max(reference_depth(schema, child)?);
    }
    Ok(maximum)
}
