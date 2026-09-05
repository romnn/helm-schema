use color_eyre::eyre;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::minimize_schema;

fn payload() -> Value {
    json!({"type": "string", "minLength": 3, "description": "Retained description. ".repeat(20)})
}

fn equivalent_validation(before: &Value, after: &Value, inputs: &[Value]) -> eyre::Result<()> {
    let original = jsonschema::validator_for(before)?;
    let minimized = jsonschema::validator_for(after)?;
    for input in inputs {
        sim_assert_eq!(have: minimized.is_valid(input), want: original.is_valid(input));
    }
    Ok(())
}

#[test]
fn unrelated_local_reference_does_not_block_root_logical_normalization() -> eyre::Result<()> {
    let reference = json!({"$ref": "#/$defs/payload"});
    let schema = json!({
        "$defs": {"payload": payload()},
        "allOf": [{"allOf": [reference.clone()]}, reference.clone()]
    });
    let minimized = minimize_schema(schema.clone());
    let expected = json!({
        "$defs": {"payload": payload()},
        "allOf": [reference]
    });
    sim_assert_eq!(have: &minimized, want: &expected);
    equivalent_validation(&schema, &minimized, &[json!("abc"), json!("a"), json!(4)])
}

#[test]
fn references_through_data_positions_leave_the_document_unchanged() -> eyre::Result<()> {
    let carrier = json!({"$ref": "#/properties/a/properties/value"});
    for (keyword, data, reference) in [
        ("default", carrier.clone(), "#/default"),
        ("examples", json!([carrier.clone()]), "#/examples/0"),
        ("x-carrier", carrier, "#/x-carrier"),
    ] {
        let repeated = json!({"type": "object", "properties": {"value": payload()}});
        let schema = json!({
            (keyword): data,
            "allOf": [{"$ref": reference}],
            "properties": {"a": repeated, "b": repeated}
        });
        let minimized = minimize_schema(schema.clone());
        sim_assert_eq!(have: &minimized, want: &schema);
        let validator = jsonschema::validator_for(&minimized)?;
        sim_assert_eq!(have: validator.is_valid(&json!("abc")), want: true);
        sim_assert_eq!(have: validator.is_valid(&json!(4)), want: false);
        equivalent_validation(&schema, &minimized, &[json!("abc"), json!("a"), json!(4)])?;
    }
    Ok(())
}

#[test]
fn reference_siblings_remain_visible_to_definition_sharing() -> eyre::Result<()> {
    let repeated = payload();
    let schema = json!({
        "$defs": {"left": repeated, "right": repeated},
        "$ref": "#/$defs/left"
    });
    let minimized = minimize_schema(schema.clone());
    let expected = json!({
        "$defs": {"1": repeated, "left": {"$ref": "#/$defs/1"}, "right": {"$ref": "#/$defs/1"}},
        "$ref": "#/$defs/left"
    });
    sim_assert_eq!(have: &minimized, want: &expected);
    equivalent_validation(&schema, &minimized, &[json!("abc"), json!("a"), json!(4)])
}

#[test]
fn existing_definition_children_are_shared() -> eyre::Result<()> {
    let repeated = payload();
    let schema = json!({
        "$defs": {
            "left": {"title": "Left", "properties": {"item": repeated}},
            "right": {"title": "Right", "properties": {"item": repeated}}
        },
        "allOf": [{"$ref": "#/$defs/left"}, {"$ref": "#/$defs/right"}]
    });
    let minimized = minimize_schema(schema.clone());
    let expected = json!({
        "$defs": {
            "1": repeated,
            "left": {"title": "Left", "properties": {"item": {"$ref": "#/$defs/1"}}},
            "right": {"title": "Right", "properties": {"item": {"$ref": "#/$defs/1"}}}
        },
        "allOf": [{"$ref": "#/$defs/left"}, {"$ref": "#/$defs/right"}]
    });
    sim_assert_eq!(have: &minimized, want: &expected);
    equivalent_validation(
        &schema,
        &minimized,
        &[
            json!({"item": "abc"}),
            json!({"item": "a"}),
            json!({"item": 4}),
        ],
    )
}

#[test]
fn extracted_parent_bodies_share_their_children() -> eyre::Result<()> {
    let child = payload();
    let parent = json!({"properties": {"left": child, "right": child}});
    let schema = json!({"properties": {"first": parent, "second": parent}});
    let minimized = minimize_schema(schema.clone());
    let expected = json!({
        "$defs": {
            "1": {"properties": {"left": {"$ref": "#/$defs/2"}, "right": {"$ref": "#/$defs/2"}}},
            "2": child
        },
        "properties": {"first": {"$ref": "#/$defs/1"}, "second": {"$ref": "#/$defs/1"}}
    });
    sim_assert_eq!(have: &minimized, want: &expected);
    equivalent_validation(
        &schema,
        &minimized,
        &[
            json!({"first": {"left": "abc"}}),
            json!({"second": {"right": "a"}}),
        ],
    )
}

#[test]
fn nested_pointer_addresses_survive_definition_minimization() -> eyre::Result<()> {
    let repeated = json!({"properties": {"value": payload()}});
    let schema = json!({
        "$defs": {"a/b~c": {"allOf": [repeated.clone(), repeated]}},
        "allOf": [{"$ref": "#/$defs/a~1b~0c/allOf/1/properties/value"}]
    });
    let minimized = minimize_schema(schema.clone());
    eyre::ensure!(
        minimized
            .pointer("/$defs/a~1b~0c/allOf/1/properties/value")
            .is_some()
    );
    equivalent_validation(&schema, &minimized, &[json!("abc"), json!("a"), json!(4)])
}

#[test]
fn percent_encoded_pointer_preserves_repeated_ancestors() -> eyre::Result<()> {
    let repeated = json!({"properties": {"value": payload()}});
    let reference = "#%2F$defs%2FA%2Fproperties%2Fvalue";
    let schema = json!({
        "$defs": {"A": repeated.clone(), "B": repeated},
        "allOf": [{"$ref": reference}]
    });
    let minimized = minimize_schema(schema.clone());
    let expected = json!({
        "$defs": {
            "1": payload(),
            "A": {"properties": {"value": {"$ref": "#/$defs/1"}}},
            "B": {"properties": {"value": {"$ref": "#/$defs/1"}}}
        },
        "allOf": [{"$ref": reference}]
    });
    sim_assert_eq!(have: &minimized, want: &expected);
    // The validator treats an encoded leading slash as an anchor.
    // Validate the equivalent decoded URI after checking the exact encoded output.
    let mut decoded_schema = schema;
    let mut decoded_minimized = minimized;
    decoded_schema["allOf"][0]["$ref"] = json!("#/$defs/A/properties/value");
    decoded_minimized["allOf"][0]["$ref"] = json!("#/$defs/A/properties/value");
    equivalent_validation(
        &decoded_schema,
        &decoded_minimized,
        &[json!("abc"), json!("a"), json!(4)],
    )
}

#[test]
fn nested_id_scope_does_not_receive_document_root_references() {
    let repeated = payload();
    let scoped = json!({
        "$id": "nested.json",
        "properties": {"first": repeated, "second": repeated}
    });
    let schema = json!({"$id": "https://example.test/root.json", "properties": {"scoped": scoped}});
    sim_assert_eq!(have: minimize_schema(schema.clone()), want: schema);
}

#[test]
fn containing_parent_does_not_relocate_a_nested_dialect() {
    let parent = json!({"properties": {"scoped": {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "properties": {"a": payload(), "b": payload()}
    }}});
    let schema = json!({"properties": {"first": parent, "second": parent}});
    sim_assert_eq!(have: minimize_schema(schema.clone()), want: schema);
}

#[test]
fn nested_resource_numeric_references_do_not_name_generated_definitions() -> eyre::Result<()> {
    let child = payload();
    let parent = json!({"properties": {"a": child, "b": child, "c": child}});
    let scoped = json!({
        "$id": "nested.json", "$defs": {"1": {"type": "integer"}}, "$ref": "#/$defs/1"
    });
    let schema = json!({
        "$id": "https://example.test/root.json",
        "properties": {"first": parent, "second": parent, "scoped": scoped}
    });
    let minimized = minimize_schema(schema.clone());
    let expected = json!({
        "$id": "https://example.test/root.json",
        "$defs": {
            "1": child,
            "2": {"properties": {"a": {"$ref": "#/$defs/1"}, "b": {"$ref": "#/$defs/1"}, "c": {"$ref": "#/$defs/1"}}}
        },
        "properties": {"first": {"$ref": "#/$defs/2"}, "second": {"$ref": "#/$defs/2"}, "scoped": scoped}
    });
    sim_assert_eq!(have: &minimized, want: &expected);
    equivalent_validation(
        &schema,
        &minimized,
        &[
            json!({"scoped": 4}),
            json!({"scoped": "abc"}),
            json!({"first": {"a": "a"}}),
        ],
    )
}

#[test]
fn external_references_keep_their_document_addresses() {
    let repeated = payload();
    let schema = json!({
        "$id": "https://example.test/root.json",
        "properties": {"first": repeated, "second": repeated},
        "allOf": [{"$ref": "https://example.test/root.json#/properties/first"}]
    });
    sim_assert_eq!(have: minimize_schema(schema.clone()), want: schema);
}

#[test]
fn undefined_reference_names_are_not_accidentally_bound() {
    let repeated = payload();
    let schema = json!({"properties": {"first": repeated, "second": repeated}, "$defs": {"unused": {"$ref": "#/$defs/1"}}});
    sim_assert_eq!(have: minimize_schema(schema.clone()), want: schema);
}

#[test]
fn complete_definition_bodies_share_while_original_names_remain() -> eyre::Result<()> {
    let repeated = payload();
    let schema = json!({
        "$defs": {"left": repeated, "right": repeated},
        "properties": {"left": {"$ref": "#/$defs/left"}, "right": {"$ref": "#/$defs/right"}}
    });
    let minimized = minimize_schema(schema.clone());
    let expected = json!({
        "$defs": {"1": repeated, "left": {"$ref": "#/$defs/1"}, "right": {"$ref": "#/$defs/1"}},
        "properties": {"left": {"$ref": "#/$defs/left"}, "right": {"$ref": "#/$defs/right"}}
    });
    sim_assert_eq!(have: &minimized, want: &expected);
    sim_assert_eq!(have: minimize_schema(minimized.clone()), want: minimized.clone());
    equivalent_validation(
        &schema,
        &minimized,
        &[json!({"left": "abc"}), json!({"right": "a"})],
    )
}

#[test]
fn recursive_definition_edges_preserve_finite_tree_validation() -> eyre::Result<()> {
    let tree = json!({
        "type": "object",
        "properties": {"label": payload(), "child": {"$ref": "#/$defs/tree"}}
    });
    let schema = json!({"$defs": {"tree": tree}, "properties": {
        "first": {"$ref": "#/$defs/tree"}, "second": tree
    }});
    let minimized = minimize_schema(schema.clone());
    let expected = json!({"$defs": {"1": tree, "tree": {"$ref": "#/$defs/1"}}, "properties": {
        "first": {"$ref": "#/$defs/tree"}, "second": {"$ref": "#/$defs/1"}
    }});
    sim_assert_eq!(have: &minimized, want: &expected);
    equivalent_validation(
        &schema,
        &minimized,
        &[
            json!({"first": {"child": {"label": "abc"}}}),
            json!({"second": {"child": {"label": "a"}}}),
        ],
    )
}

#[test]
fn incoming_pointer_keeps_logical_array_positions() -> eyre::Result<()> {
    let repeated = payload();
    let schema = json!({
        "$defs": {"usesIndex": {"$ref": "#/allOf/1"}},
        "allOf": [repeated.clone(), repeated]
    });
    let minimized = minimize_schema(schema.clone());
    let expected = json!({
        "$defs": {"1": payload(), "usesIndex": {"$ref": "#/allOf/1"}},
        "allOf": [{"$ref": "#/$defs/1"}, {"$ref": "#/$defs/1"}]
    });
    sim_assert_eq!(have: &minimized, want: &expected);
    equivalent_validation(&schema, &minimized, &[json!("abc"), json!("a")])
}

#[test]
fn anchors_and_dynamic_scope_regions_remain_unchanged() {
    for keyword in ["$anchor", "$dynamicAnchor", "$recursiveAnchor"] {
        let scoped = json!({(keyword): "tree", "properties": {"a": payload(), "b": payload()}});
        let schema =
            json!({"$defs": {"scoped": scoped}, "properties": {"value": {"$ref": "#tree"}}});
        sim_assert_eq!(have: minimize_schema(schema.clone()), want: schema);
    }
}
