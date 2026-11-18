use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_json_schema_walk::DefinitionsKeyword;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::single_use::inline_single_use_definitions;

#[test]
fn single_use_inlining_preserves_verdicts_and_reaches_a_deterministic_fixpoint() -> eyre::Result<()>
{
    let mut state = 0x3267_2159_85ab_c0d3u64;
    for _ in 0..48 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let boundary = i64::try_from((state >> 32) % 41)? - 20;
        let singleton = if state & 1 == 0 { "a" } else { "z" };
        let mut prior_verdicts = None;

        for keyword in DefinitionsKeyword::ALL {
            let link = |name| json!({"$ref": keyword.reference(name)});
            let schema = json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": [singleton],
                "properties": {
                    (singleton): link("alias~/entry"),
                    "left": link("shared"),
                    "right": link("shared")
                },
                (keyword.as_str()): {
                    "alias~/entry": link("branch"),
                    "branch": {
                        "anyOf": [
                            {"type": "integer", "minimum": boundary},
                            link("flag")
                        ]
                    },
                    "flag": {"const": true},
                    "shared": {"type": "string", "minLength": 2}
                },
                "default": {"$ref": keyword.reference("flag")}
            });
            let expected = json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": [singleton],
                "properties": {
                    (singleton): {
                        "anyOf": [
                            {"type": "integer", "minimum": boundary},
                            {"const": true}
                        ]
                    },
                    "left": link("shared"),
                    "right": link("shared")
                },
                (keyword.as_str()): {"shared": {"type": "string", "minLength": 2}},
                "default": {"$ref": keyword.reference("flag")}
            });
            let mut inlined = schema.clone();
            inline_single_use_definitions(&mut inlined);
            sim_assert_eq!(have: &inlined, want: &expected);
            inline_single_use_definitions(&mut inlined);
            sim_assert_eq!(have: &inlined, want: &expected);
            let mut reverse = reverse_object_order(&schema);
            inline_single_use_definitions(&mut reverse);
            sim_assert_eq!(have: &reverse, want: &expected);

            let original = jsonschema::validator_for(&schema)?;
            let rewritten = jsonschema::validator_for(&inlined)?;
            let documents = [
                json!({}),
                json!({(singleton): boundary - 1}),
                json!({(singleton): boundary}),
                json!({(singleton): boundary + 1, "left": "a"}),
                json!({(singleton): true, "right": "ab"}),
                json!({(singleton): false, "left": "ab", "right": "ab"}),
                json!({(singleton): "integer"}),
                Value::Null,
            ];
            let verdicts = documents
                .iter()
                .map(|document| {
                    let actual = rewritten.is_valid(document);
                    sim_assert_eq!(have: actual, want: original.is_valid(document));
                    actual
                })
                .collect::<Vec<_>>();
            if let Some(prior) = &prior_verdicts {
                sim_assert_eq!(have: json!(verdicts), want: json!(prior));
            } else {
                prior_verdicts = Some(verdicts);
            }
        }
    }
    Ok(())
}

#[test]
fn recursive_pointer_and_sibling_references_preserve_their_targets() -> eyre::Result<()> {
    let mut schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "properties": {
            "outer": {"$ref": "#/$defs/outer"},
            "again": {"$ref": "#/$defs/outer"},
            "target": {"$ref": "#/$defs/addressed/properties/value"},
            "sibling": {"$ref": "#/$defs/integer", "minimum": 3},
            "inline": {"$ref": "#/$defs/once"}
        },
        "$defs": {
            "outer": {"properties": {"inner": {"$ref": "#/$defs/inner"}}},
            "inner": {"properties": {"outer": {"$ref": "#/$defs/outer"}}},
            "addressed": {"properties": {"value": {"type": "boolean"}}},
            "integer": {"type": "integer"},
            "once": {"const": 9}
        }
    });
    let before = schema.clone();
    let mut expected = before.clone();
    expected["properties"]["inline"] = json!({"const": 9});
    expected["$defs"]
        .as_object_mut()
        .ok_or_eyre("definitions")?
        .remove("once");
    inline_single_use_definitions(&mut schema);
    sim_assert_eq!(have: &schema, want: &expected);
    let original = jsonschema::validator_for(&before)?;
    let actual = jsonschema::validator_for(&schema)?;
    for document in [
        json!({"inline": 9, "outer": {"inner": {"outer": {}}}, "target": true, "sibling": 5}),
        json!({"inline": 0}),
        json!({"target": "true"}),
        json!({"sibling": 2}),
    ] {
        sim_assert_eq!(have: actual.is_valid(&document), want: original.is_valid(&document));
    }
    Ok(())
}

fn reverse_object_order(value: &Value) -> Value {
    match value {
        Value::Object(members) => Value::Object(
            members
                .iter()
                .rev()
                .map(|(name, child)| (name.clone(), reverse_object_order(child)))
                .collect(),
        ),
        Value::Array(children) => Value::Array(children.iter().map(reverse_object_order).collect()),
        _ => value.clone(),
    }
}
