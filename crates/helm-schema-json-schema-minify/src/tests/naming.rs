use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use serde_json::{Map, Value, json};
use test_util::prelude::sim_assert_eq;

use super::minimize;
use crate::{content_digest, content_names, rename_definitions, shipping_definition_names};

/// An object schema large enough that two uses are worth extracting.
fn record(label: &str, fields: &[&str]) -> Value {
    let mut properties = Map::new();
    for field in fields {
        properties.insert(
            (*field).to_string(),
            json!({ "const": format!("{label}-{field}") }),
        );
    }
    json!({
        "additionalProperties": false,
        "description": format!("The {label} record."),
        "properties": properties,
        "type": "object"
    })
}

fn definitions(schema: &Value) -> BTreeMap<String, Value> {
    schema
        .get("$defs")
        .and_then(Value::as_object)
        .map(|definitions| {
            definitions
                .iter()
                .map(|(name, body)| (name.clone(), body.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn reference_at<'a>(schema: &'a Value, pointer: &str) -> Option<&'a str> {
    schema.pointer(pointer)?.get("$ref")?.as_str()
}

#[test]
fn unrelated_insertion_keeps_existing_definition_names() {
    let alpha = record("alpha", &["one", "two", "three"]);
    let beta = record("beta", &["one", "two", "three", "four", "five"]);
    let before = minimize(json!({
        "properties": { "left": alpha, "right": alpha },
        "type": "object"
    }));
    // The inserted record is larger and more referenced, so an ordinal scheme
    // hands it the name the alpha record held before.
    let after = minimize(json!({
        "properties": {
            "left": alpha,
            "right": alpha,
            "x": beta,
            "y": beta,
            "z": beta
        },
        "type": "object"
    }));

    let before_definitions = definitions(&before);
    let after_definitions = definitions(&after);
    sim_assert_eq!(have: before_definitions.len(), want: 1);
    sim_assert_eq!(have: after_definitions.len(), want: 2);
    for (name, body) in &before_definitions {
        sim_assert_eq!(have: after_definitions.get(name), want: Some(body));
    }
    sim_assert_eq!(
        have: reference_at(&after, "/properties/left"),
        want: reference_at(&before, "/properties/left")
    );
}

#[test]
fn body_edit_renames_the_definition_and_its_dependants_only() {
    let child = record("child", &["one", "two", "three"]);
    let mut edited_child = child.clone();
    edited_child["properties"]["one"] = json!({ "const": "child-edited" });
    let parent = |child: &Value| {
        json!({
            "properties": { "inner": child, "label": { "const": "parent label text" } },
            "type": "object"
        })
    };
    let unrelated = record("unrelated", &["one", "two", "three"]);
    let document = |child: &Value| {
        json!({
            "properties": {
                "a": parent(child),
                "b": parent(child),
                "c": child,
                "u": unrelated,
                "v": unrelated
            },
            "type": "object"
        })
    };

    let before = minimize(document(&child));
    let after = minimize(document(&edited_child));

    // The edited child and the parent that contains it are renamed; the
    // unrelated definition keeps its name and body.
    for pointer in ["/properties/a", "/properties/c"] {
        assert!(
            reference_at(&before, pointer).is_some(),
            "{pointer} should be extracted"
        );
        assert_ne!(
            reference_at(&after, pointer),
            reference_at(&before, pointer),
            "{pointer} must be renamed"
        );
    }
    sim_assert_eq!(
        have: reference_at(&after, "/properties/u"),
        want: reference_at(&before, "/properties/u")
    );
    sim_assert_eq!(have: definitions(&after).len(), want: definitions(&before).len());
}

#[test]
fn description_edit_renames_the_definition() {
    let original = record("alpha", &["one", "two", "three"]);
    let mut redescribed = original.clone();
    redescribed["description"] = json!("A different description.");
    let document = |record: &Value| json!({ "properties": { "left": record, "right": record }, "type": "object" });

    let before = minimize(document(&original));
    let after = minimize(document(&redescribed));

    assert!(reference_at(&before, "/properties/left").is_some());
    assert_ne!(
        reference_at(&after, "/properties/left"),
        reference_at(&before, "/properties/left")
    );
}

#[test]
fn cycle_through_a_named_helper_keeps_its_names() {
    let helper_reference = json!({ "$ref": "#/$defs/helm-double-quoted-safe" });
    let helper = |pattern: &str| {
        json!({
            "anyOf": [
                { "type": ["boolean", "integer", "null", "number"] },
                { "pattern": pattern, "type": "string" },
                { "additionalProperties": helper_reference, "type": "object" },
                { "items": helper_reference, "type": "array" }
            ]
        })
    };
    let document = |helper: Value, extra: Option<Value>| {
        let mut properties = Map::new();
        for name in ["a", "b", "c", "d"] {
            properties.insert(name.to_string(), helper_reference.clone());
        }
        // More uses than the helper reference, so an ordinal scheme reorders.
        if let Some(extra) = extra {
            for name in ["s", "t", "u", "v", "w", "x", "y", "z"] {
                properties.insert(name.to_string(), extra.clone());
            }
        }
        json!({
            "$defs": { "helm-double-quoted-safe": helper },
            "properties": properties,
            "type": "object"
        })
    };

    let before = minimize(document(helper("^[a-z]*$"), None));
    let inserted = minimize(document(
        helper("^[a-z]*$"),
        Some(record("beta", &["one", "two", "three", "four", "five"])),
    ));
    // The helper is an identity boundary: its callers name it, not its body.
    let helper_edited = minimize(document(helper("^[A-Z]*$"), None));

    let generated = reference_at(&before, "/properties/a");
    assert!(
        generated.is_some_and(|reference| reference != "#/$defs/helm-double-quoted-safe"),
        "the helper reference itself should be extracted: {before}"
    );
    sim_assert_eq!(have: reference_at(&inserted, "/properties/a"), want: generated);
    sim_assert_eq!(have: reference_at(&helper_edited, "/properties/a"), want: generated);
}

#[test]
fn unrelated_insertion_output_is_pinned() {
    let alpha = record("alpha", &["one", "two", "three"]);
    let beta = record("beta", &["one", "two", "three", "four", "five"]);
    let after = minimize(json!({
        "properties": { "left": alpha, "right": alpha, "x": beta, "y": beta, "z": beta },
        "type": "object"
    }));

    sim_assert_eq!(
        have: after,
        want: json!({
            "$defs": { "h9081c41d3351": alpha, "h9ac9edc9ffde": beta },
            "properties": {
                "left": { "$ref": "#/$defs/h9081c41d3351" },
                "right": { "$ref": "#/$defs/h9081c41d3351" },
                "x": { "$ref": "#/$defs/h9ac9edc9ffde" },
                "y": { "$ref": "#/$defs/h9ac9edc9ffde" },
                "z": { "$ref": "#/$defs/h9ac9edc9ffde" }
            },
            "type": "object"
        })
    );
}

fn string_map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(id, digest)| ((*id).to_string(), (*digest).to_string()))
        .collect()
}

#[test]
fn prefix_collision_extends_only_the_colliding_group() {
    let digests = string_map(&[
        ("a", "0123456789abcd00ffff"),
        ("b", "0123456789abce00ffff"),
        ("c", "0123456789abcf00ffff"),
        ("d", "fedcba9876543210ffff"),
    ]);

    let assigned = content_names("p-", &digests, &BTreeSet::new());

    // `a`, `b` and `c` share twelve digits and extend to thirteen; `d` keeps
    // the fixed twelve-digit name.
    sim_assert_eq!(
        have: assigned,
        want: string_map(&[
            ("a", "p-0123456789abcd"),
            ("b", "p-0123456789abce"),
            ("c", "p-0123456789abcf"),
            ("d", "p-fedcba987654"),
        ])
    );
}

#[test]
fn content_names_avoid_taken_names_and_keep_equal_digests_distinct() {
    let digests = string_map(&[
        ("a", "0123456789abcdef"),
        ("b", "0123456789abcdef"),
        ("c", "fedcba9876543210"),
    ]);
    let taken = BTreeSet::from(["p-fedcba987654".to_string()]);

    let assigned = content_names("p-", &digests, &taken);

    sim_assert_eq!(
        have: assigned,
        want: string_map(&[
            ("a", "p-0123456789abcdef"),
            ("b", "p-0123456789abcdef-2"),
            ("c", "p-fedcba9876543"),
        ])
    );
}

#[test]
fn content_digest_includes_descriptions_and_ignores_key_order() -> eyre::Result<()> {
    let described = json!({ "description": "one", "type": "string" });
    let reordered: Value = serde_json::from_str(r#"{"type":"string","description":"one"}"#)?;
    let redescribed = json!({ "description": "two", "type": "string" });

    sim_assert_eq!(have: content_digest(&reordered), want: content_digest(&described));
    assert_ne!(content_digest(&redescribed), content_digest(&described));
    Ok(())
}

/// A readable document with self and mutual cycles, escaped names, a pointer
/// suffix, a `$ref` sibling, and `$ref`-looking data.
fn readable_document() -> Value {
    json!({
        "$defs": {
            "a/b": { "items": { "$ref": "#/$defs/a~1b" }, "type": "array" },
            "c~d": { "properties": { "next": { "$ref": "#/$defs/hmutual" } } },
            "hmutual": {
                "properties": { "back": { "$ref": "#/$defs/c~0d" } },
                "default": { "$ref": "#/$defs/hmutual" }
            },
            "hleaf": { "properties": { "name": { "type": "string" } } }
        },
        "properties": {
            "cycle": { "$ref": "#/$defs/a~1b" },
            "mutual": { "$ref": "#/$defs/c~0d" },
            "leaf": { "$ref": "#/$defs/hleaf/properties/name" },
            "leaf2": { "$ref": "#/$defs/hleaf" },
            "leaf3": { "$ref": "#/$defs/hleaf", "description": "A sibling annotation." },
            "data": { "examples": [{ "$ref": "#/$defs/hleaf" }], "const": "#/$defs/hleaf" }
        }
    })
}

#[test]
fn shipping_rename_is_a_bijection_that_inverts_to_the_readable_document() -> eyre::Result<()> {
    let readable = readable_document();
    let renames = shipping_definition_names(&readable);
    let mut shipped = readable.clone();
    rename_definitions(&mut shipped, &renames);

    // Total coverage, unique destinations, equal definition counts.
    sim_assert_eq!(
        have: renames.keys().cloned().collect::<BTreeSet<_>>(),
        want: definitions(&readable).into_keys().collect::<BTreeSet<_>>()
    );
    sim_assert_eq!(
        have: renames.values().collect::<BTreeSet<_>>().len(),
        want: renames.len()
    );
    sim_assert_eq!(have: definitions(&shipped).len(), want: definitions(&readable).len());

    // The most referenced definition gets the shortest name.
    sim_assert_eq!(
        have: shipped,
        want: json!({
            "$defs": {
                "1": { "properties": { "name": { "type": "string" } } },
                "2": { "items": { "$ref": "#/$defs/2" }, "type": "array" },
                "3": { "properties": { "next": { "$ref": "#/$defs/4" } } },
                "4": {
                    "properties": { "back": { "$ref": "#/$defs/3" } },
                    "default": { "$ref": "#/$defs/hmutual" }
                }
            },
            "properties": {
                "cycle": { "$ref": "#/$defs/2" },
                "mutual": { "$ref": "#/$defs/3" },
                "leaf": { "$ref": "#/$defs/1/properties/name" },
                "leaf2": { "$ref": "#/$defs/1" },
                "leaf3": { "$ref": "#/$defs/1", "description": "A sibling annotation." },
                "data": { "examples": [{ "$ref": "#/$defs/hleaf" }], "const": "#/$defs/hleaf" }
            }
        })
    );

    // Every rewritten reference resolves in the shipped document.
    for pointer in [
        "/properties/cycle",
        "/properties/mutual",
        "/properties/leaf",
        "/properties/leaf3",
        "/$defs/2/items",
        "/$defs/3/properties/next",
        "/$defs/4/properties/back",
    ] {
        let reference = reference_at(&shipped, pointer).ok_or_eyre("reference expected")?;
        let target = reference
            .strip_prefix('#')
            .ok_or_eyre("local reference expected")?;
        assert!(shipped.pointer(target).is_some(), "{reference} dangles");
    }

    let inverse = renames
        .iter()
        .map(|(readable, shipped)| (shipped.clone(), readable.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut restored = shipped;
    rename_definitions(&mut restored, &inverse);
    sim_assert_eq!(have: restored, want: readable);
    Ok(())
}

#[test]
fn shipping_rename_abstains_on_references_it_cannot_rewrite() {
    let external = json!({
        "$defs": { "hleaf": { "type": "string" } },
        "properties": { "a": { "$ref": "other.json#/$defs/hleaf" } }
    });
    let encoded = json!({
        "$defs": { "hleaf": { "type": "string" } },
        "properties": { "a": { "$ref": "#/%24defs/hleaf" } }
    });
    let dangling = json!({
        "$defs": { "hleaf": { "type": "string" } },
        "properties": { "a": { "$ref": "#/$defs/1" } }
    });

    // `$dynamicRef` to a JSON Pointer behaves like `$ref`, which the rename
    // would leave dangling.
    let dynamic = json!({
        "$defs": { "hlong-name": { "type": "string" } },
        "properties": { "a": { "$dynamicRef": "#/$defs/hlong-name" } }
    });
    // A fragment-only `$id` names a location, not a new resource, so the
    // reference below it still resolves against the root.
    let fragment_identifier = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {},
        "$defs": { "hlong-name": { "type": "string" } },
        "properties": {
            "a": { "$id": "#entry", "properties": { "b": { "$ref": "#/$defs/hlong-name" } } }
        }
    });
    let nested_resource = json!({
        "$defs": { "hleaf": { "type": "string" } },
        "properties": {
            "a": { "$ref": "#/$defs/hleaf" },
            "b": { "$id": "nested.json", "$defs": { "hleaf": {} }, "$ref": "#/$defs/hleaf" }
        }
    });
    let anchor = json!({
        "$defs": { "hlong-name": { "$anchor": "leaf", "type": "string" } },
        "properties": { "a": { "$ref": "#leaf" } }
    });

    for schema in [
        external,
        encoded,
        dangling,
        dynamic,
        fragment_identifier,
        nested_resource,
        anchor,
    ] {
        sim_assert_eq!(have: shipping_definition_names(&schema), want: BTreeMap::new());
    }
}

#[test]
fn extracting_an_inline_child_keeps_the_parent_name() {
    let child = record("child", &["one", "two", "three"]);
    let parent = json!({
        "properties": { "inner": child, "label": { "const": "parent label text" } },
        "type": "object"
    });
    // The child occurs once per parent use: it stays inline.
    let inline = minimize(json!({
        "properties": { "a": parent, "b": parent },
        "type": "object"
    }));
    // Two more uses make the child worth extracting inside the parent body.
    let extracted = minimize(json!({
        "properties": { "a": parent, "b": parent, "c": child, "d": child },
        "type": "object"
    }));

    sim_assert_eq!(have: definitions(&inline).len(), want: 1);
    sim_assert_eq!(have: definitions(&extracted).len(), want: 2);
    sim_assert_eq!(
        have: reference_at(&extracted, "/properties/a"),
        want: reference_at(&inline, "/properties/a")
    );
}

#[test]
fn unrelated_conditional_arm_insertion_keeps_existing_definition_names() {
    let alpha = record("alpha", &["one", "two", "three"]);
    let beta = record("beta", &["one", "two", "three", "four", "five"]);
    let arm = |flag: &str, record: &Value| {
        json!({
            "if": { "properties": { flag: { "const": true } }, "required": [flag] },
            "then": { "properties": { "left": record, "right": record, "extra": record } }
        })
    };
    let before = minimize(json!({ "allOf": [arm("alpha", &alpha)], "type": "object" }));
    let after = minimize(json!({
        "allOf": [arm("beta", &beta), arm("alpha", &alpha)],
        "type": "object"
    }));

    let after_definitions = definitions(&after);
    sim_assert_eq!(have: after_definitions.len(), want: 2);
    for (name, body) in definitions(&before) {
        sim_assert_eq!(have: after_definitions.get(&name), want: Some(&body));
    }
}

#[test]
fn private_handles_decide_extraction_before_their_final_names_apply() {
    let document = |name: &str| {
        let property = json!({
            "allOf": [{ "$ref": format!("#/$defs/{name}") }],
            "description": "short"
        });
        json!({
            "$defs": { name: { "type": "string" } },
            "properties": { "a": property, "b": property }
        })
    };
    let readable = "providerSchema_0123456789ab";
    let names = BTreeMap::from([("p1".to_string(), readable.to_string())]);

    // Measured with the private handle, sharing the two uses does not pay,
    // so the final name only replaces the handle.
    sim_assert_eq!(
        have: crate::minimize_schema(document("p1"), &names),
        want: document(readable)
    );
    // Measured with the readable spelling, the same subtree would move.
    sim_assert_eq!(have: definitions(&minimize(document(readable))).len(), want: 2);
}

#[test]
fn private_names_skip_absent_handles_and_taken_names() {
    let mut schema = json!({
        "$defs": { "p1": { "type": "string" }, "p2": { "type": "null" }, "taken": {} },
        "properties": { "a": { "$ref": "#/$defs/p1" }, "b": { "$ref": "#/$defs/p2" } }
    });
    let names = BTreeMap::from([
        ("p1".to_string(), "final-one".to_string()),
        ("p2".to_string(), "taken".to_string()),
        ("p3".to_string(), "final-three".to_string()),
    ]);

    crate::name_private_definitions(&mut schema, &names);

    sim_assert_eq!(
        have: schema,
        want: json!({
            "$defs": { "final-one": { "type": "string" }, "p2": { "type": "null" }, "taken": {} },
            "properties": { "a": { "$ref": "#/$defs/final-one" }, "b": { "$ref": "#/$defs/p2" } }
        })
    );
}
