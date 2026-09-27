use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use serde_json::{Map, Value, json};
use test_util::prelude::sim_assert_eq;

use super::minimize;
use crate::{
    DefinitionNames, DefinitionOrigin, expand_short_definition_names, minimize_schema,
    name_definitions, rename_definitions, shorten_definition_names,
};

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

fn origin(provider: &str, document: &str, pointer: &str) -> DefinitionOrigin {
    DefinitionOrigin {
        provider: provider.to_string(),
        document: document.to_string(),
        pointer: pointer.to_string(),
    }
}

/// Private handles as a generator hands them over: two provider definitions
/// with recorded origins, one shared payload without one.
fn handles() -> BTreeMap<String, Vec<DefinitionOrigin>> {
    BTreeMap::from([
        (
            "providerSource_k8s_0123456789ab".to_string(),
            vec![origin(
                "k8s",
                "_definitions.json",
                "/definitions/io.k8s.api.core.v1.SecurityContext",
            )],
        ),
        (
            "providerSchema1".to_string(),
            vec![
                origin(
                    "crd",
                    "monitoring.coreos.com/prometheus_v1.json",
                    "/properties/spec/properties/containers/items",
                ),
                origin(
                    "crd",
                    "monitoring.coreos.com/alertmanager_v1.json",
                    "/properties/spec/properties/containers/items",
                ),
            ],
        ),
        ("providerShared1".to_string(), Vec::new()),
        ("7".to_string(), Vec::new()),
        ("8".to_string(), Vec::new()),
        ("9".to_string(), Vec::new()),
    ])
}

/// A document over those handles: a named helper, a quoted property name,
/// and references below `items`, `anyOf`, `additionalProperties` and
/// `patternProperties`.
fn handle_document() -> Value {
    json!({
        "$defs": {
            "helm-double-quoted-safe": {
                "anyOf": [{ "type": "string" }, { "items": { "$ref": "#/$defs/9" } }]
            },
            "providerSource_k8s_0123456789ab": {
                "properties": { "capabilities": { "$ref": "#/$defs/7" } }
            },
            "providerSchema1": {
                "properties": { "securityContext": { "$ref": "#/$defs/providerSource_k8s_0123456789ab" } }
            },
            "providerShared1": { "properties": { "drop": { "$ref": "#/$defs/8" } } },
            "7": { "properties": { "add": { "$ref": "#/$defs/8" } } },
            "8": { "items": { "type": "string" }, "type": "array" },
            "9": { "type": "integer" }
        },
        "properties": {
            "web": {
                "properties": {
                    "sidecars": { "items": { "$ref": "#/$defs/providerSchema1" } },
                    "app.kubernetes.io/name": { "$ref": "#/$defs/providerShared1" }
                }
            },
            "api": {
                "anyOf": [
                    { "type": "null" },
                    { "additionalProperties": { "$ref": "#/$defs/7" } }
                ],
                "patternProperties": { "^x-": { "$ref": "#/$defs/helm-double-quoted-safe" } }
            }
        },
        "type": "object"
    })
}

/// `handle_document` with every definition key and reference renamed.
fn renamed_document(names: &[(&str, &str)]) -> Value {
    let mut document = handle_document();
    let renames = names
        .iter()
        .map(|(handle, name)| ((*handle).to_string(), (*name).to_string()))
        .collect();
    rename_definitions(&mut document, &renames);
    document
}

#[test]
fn source_names_are_recorded_origins_or_the_smallest_referencing_path() {
    let mut schema = handle_document();

    name_definitions(&mut schema, &handles(), DefinitionNames::Source);

    // The k8s pointer names an OpenAPI type; the CRD definition takes the
    // smaller of its two origins; `7` is referenced from `values/api…` and
    // from inside the k8s type, which sorts first; `8` is reached only
    // through anonymous definitions and takes the smaller of their paths;
    // `9` sits below the named helper.
    sim_assert_eq!(
        have: schema,
        want: renamed_document(&[
            ("providerSource_k8s_0123456789ab", "k8s/io.k8s.api.core.v1.SecurityContext"),
            (
                "providerSchema1",
                "crd/monitoring.coreos.com/alertmanager_v1/spec.containers@items"
            ),
            ("providerShared1", "values/web.'app.kubernetes.io/name'"),
            ("7", "k8s/io.k8s.api.core.v1.SecurityContext.capabilities"),
            ("8", "k8s/io.k8s.api.core.v1.SecurityContext.capabilities.add"),
            ("9", "helm-double-quoted-safe@anyOf(1)@items"),
        ])
    );
}

#[test]
fn destination_names_are_the_first_reference_in_canonical_order() {
    let mut schema = handle_document();

    name_definitions(&mut schema, &handles(), DefinitionNames::Destination);

    // Keys are visited in sorted order, so `api` comes before `web`, and a
    // definition's references are visited only after the whole document.
    sim_assert_eq!(
        have: schema,
        want: renamed_document(&[
            ("7", "api@anyOf(1)@additionalProperties"),
            ("providerShared1", "web.'app.kubernetes.io/name'"),
            ("providerSchema1", "web.sidecars@items"),
            ("8", "api@anyOf(1)@additionalProperties.add"),
            ("9", "helm-double-quoted-safe@anyOf(1)@items"),
            ("providerSource_k8s_0123456789ab", "web.sidecars@items.securityContext"),
        ])
    );
}

#[test]
fn readable_names_resolve_as_references() -> eyre::Result<()> {
    let mut schema = handle_document();
    name_definitions(&mut schema, &handles(), DefinitionNames::Source);
    let validator = jsonschema::validator_for(&schema)?;

    assert!(validator.is_valid(&json!({ "web": { "app.kubernetes.io/name": { "drop": ["a"] } } })));
    assert!(!validator.is_valid(&json!({ "web": { "app.kubernetes.io/name": { "drop": [1] } } })));
    assert!(!validator.is_valid(&json!({ "api": { "x-a": [true] } })));
    Ok(())
}

#[test]
fn naming_is_deterministic_and_independent_of_key_order() -> eyre::Result<()> {
    let forward: Value = serde_json::from_str(indoc::indoc! {r##"
        {"properties": {"a": {"properties": {"x": {"$ref": "#/$defs/1"}}}, "b": {"$ref": "#/$defs/1"}},
         "$defs": {"1": {"type": "string"}}}
    "##})?;
    let backward: Value = serde_json::from_str(indoc::indoc! {r##"
        {"$defs": {"1": {"type": "string"}},
         "properties": {"b": {"$ref": "#/$defs/1"}, "a": {"properties": {"x": {"$ref": "#/$defs/1"}}}}}
    "##})?;
    let anonymous = BTreeMap::from([("1".to_string(), Vec::new())]);
    for policy in [DefinitionNames::Source, DefinitionNames::Destination] {
        let mut runs = Vec::new();
        for document in [&forward, &forward, &backward] {
            let mut schema = document.clone();
            name_definitions(&mut schema, &anonymous, policy);
            runs.push(schema);
        }
        sim_assert_eq!(have: &runs[1], want: &runs[0]);
        sim_assert_eq!(have: &runs[2], want: &runs[0]);
        sim_assert_eq!(have: definitions(&runs[0]).into_keys().collect::<Vec<_>>(), want: vec![match policy {
            DefinitionNames::Source => "values/a.x".to_string(),
            DefinitionNames::Destination => "a.x".to_string(),
        }]);
    }
    Ok(())
}

#[test]
fn an_unrelated_values_edit_keeps_existing_names() {
    let alpha = record("alpha", &["one", "two", "three"]);
    let beta = record("beta", &["one", "two", "three", "four", "five"]);
    let before = minimize(json!({
        "properties": { "left": alpha, "right": alpha, "x": beta, "y": beta },
        "type": "object"
    }));
    // A new repeated record and a new plain property, elsewhere in values.
    let gamma = record("gamma", &["one", "two", "three", "four"]);
    let after = minimize(json!({
        "properties": {
            "left": alpha, "right": alpha, "x": beta, "y": beta,
            "zeta": { "properties": { "g1": gamma, "g2": gamma, "count": { "type": "integer" } } }
        },
        "type": "object"
    }));

    let after_definitions = definitions(&after);
    sim_assert_eq!(
        have: after_definitions.keys().cloned().collect::<Vec<_>>(),
        want: vec!["values/left", "values/x", "values/zeta.g1"]
    );
    for (name, body) in definitions(&before) {
        sim_assert_eq!(have: after_definitions.get(&name), want: Some(&body));
    }
}

#[test]
fn an_earlier_use_of_a_provider_type_keeps_its_source_name() {
    let document = |uses: &[&str]| {
        let mut properties = Map::new();
        for name in uses {
            properties.insert(
                (*name).to_string(),
                json!({ "$ref": "#/$defs/providerSource_k8s_0123456789ab" }),
            );
        }
        json!({
            "$defs": { "providerSource_k8s_0123456789ab": { "type": "object" } },
            "properties": properties
        })
    };
    let anonymous = BTreeMap::from([(
        "providerSource_k8s_0123456789ab".to_string(),
        vec![origin(
            "k8s",
            "_definitions.json",
            "/definitions/io.k8s.api.core.v1.Container",
        )],
    )]);
    let name = |uses: &[&str], policy| {
        let mut schema = document(uses);
        name_definitions(&mut schema, &anonymous, policy);
        definitions(&schema).into_keys().collect::<Vec<_>>()
    };

    let source = vec!["k8s/io.k8s.api.core.v1.Container".to_string()];
    sim_assert_eq!(have: name(&["web"], DefinitionNames::Source), want: source.clone());
    sim_assert_eq!(have: name(&["api", "web"], DefinitionNames::Source), want: source);
    // A destination name follows the first use instead.
    sim_assert_eq!(have: name(&["web"], DefinitionNames::Destination), want: vec!["web".to_string()]);
    sim_assert_eq!(
        have: name(&["api", "web"], DefinitionNames::Destination),
        want: vec!["api".to_string()]
    );
}

#[test]
fn minimized_output_is_pinned() {
    let alpha = record("alpha", &["one", "two", "three"]);
    let beta = record("beta", &["one", "two", "three", "four", "five"]);
    let after = minimize(json!({
        "properties": { "left": alpha, "right": alpha, "x": beta, "y": beta, "z": beta },
        "type": "object"
    }));

    sim_assert_eq!(
        have: after,
        want: json!({
            "$defs": { "values/left": alpha, "values/x": beta },
            "properties": {
                "left": { "$ref": "#/$defs/values~1left" },
                "right": { "$ref": "#/$defs/values~1left" },
                "x": { "$ref": "#/$defs/values~1x" },
                "y": { "$ref": "#/$defs/values~1x" },
                "z": { "$ref": "#/$defs/values~1x" }
            },
            "type": "object"
        })
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
    let helper_edited = minimize(document(helper("^[A-Z]*$"), None));

    // The helper reference also occurs inside the helper, whose path sorts
    // before `values/a`.
    let name = Some("#/$defs/helm-double-quoted-safe@anyOf(1)@additionalProperties");
    sim_assert_eq!(have: reference_at(&before, "/properties/a"), want: name);
    sim_assert_eq!(have: reference_at(&inserted, "/properties/a"), want: name);
    sim_assert_eq!(have: reference_at(&helper_edited, "/properties/a"), want: name);
    assert!(definitions(&before).contains_key("helm-double-quoted-safe"));
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

    sim_assert_eq!(
        have: definitions(&inline).into_keys().collect::<Vec<_>>(),
        want: vec!["values/a"]
    );
    sim_assert_eq!(
        have: definitions(&extracted).into_keys().collect::<Vec<_>>(),
        want: vec!["values/a", "values/a.inner"]
    );
}

#[test]
fn private_handles_decide_extraction_before_their_final_names_apply() {
    let document = |name: &str| {
        let property = json!({
            "allOf": [{ "$ref": format!("#/$defs/{}", name.replace('/', "~1")) }],
            "description": "short"
        });
        json!({
            "$defs": { name: { "type": "string" } },
            "properties": { "a": property, "b": property }
        })
    };
    let readable = "k8s/io.k8s.apimachinery.pkg.util.intstr.IntOrString";
    let anonymous = BTreeMap::from([(
        "p1".to_string(),
        vec![origin(
            "k8s",
            "_definitions.json",
            "/definitions/io.k8s.apimachinery.pkg.util.intstr.IntOrString",
        )],
    )]);

    // Measured with the private handle, sharing the two uses does not pay,
    // so the final name only replaces the handle.
    sim_assert_eq!(
        have: minimize_schema(document("p1"), &anonymous, DefinitionNames::Source),
        want: document(readable)
    );
    // Measured with the readable spelling, the same subtree would move.
    sim_assert_eq!(have: definitions(&minimize(document(readable))).len(), want: 2);
}

#[test]
fn naming_skips_absent_handles_and_suffixes_taken_names() {
    let mut schema = json!({
        "$defs": {
            "p1": { "type": "string" },
            "p2": { "type": "null" },
            "values/b": { "type": "boolean" }
        },
        "properties": {
            "a": { "$ref": "#/$defs/p1" },
            "b": { "$ref": "#/$defs/p2" },
            "c": { "$ref": "#/$defs/values~1b" }
        }
    });
    let anonymous = BTreeMap::from([
        ("p1".to_string(), Vec::new()),
        ("p2".to_string(), Vec::new()),
        ("p3".to_string(), Vec::new()),
    ]);

    name_definitions(&mut schema, &anonymous, DefinitionNames::Source);

    sim_assert_eq!(
        have: schema,
        want: json!({
            "$defs": {
                "values/a": { "type": "string" },
                "values/b@2": { "type": "null" },
                "values/b": { "type": "boolean" }
            },
            "properties": {
                "a": { "$ref": "#/$defs/values~1a" },
                "b": { "$ref": "#/$defs/values~1b@2" },
                "c": { "$ref": "#/$defs/values~1b" }
            }
        })
    );
}

/// A readable document with self and mutual cycles, escaped names, a pointer
/// suffix, a `$ref` sibling, and `$ref`-looking data.
fn readable_document() -> Value {
    json!({
        "$defs": {
            "values/cycle": { "items": { "$ref": "#/$defs/values~1cycle" }, "type": "array" },
            "c~d": { "properties": { "next": { "$ref": "#/$defs/values~1mutual" } } },
            "values/mutual": {
                "properties": { "back": { "$ref": "#/$defs/c~0d" } },
                "default": { "$ref": "#/$defs/values~1mutual" }
            },
            "values/leaf": { "properties": { "name": { "type": "string" } } }
        },
        "properties": {
            "cycle": { "$ref": "#/$defs/values~1cycle" },
            "mutual": { "$ref": "#/$defs/c~0d" },
            "leaf": { "$ref": "#/$defs/values~1leaf/properties/name" },
            "leaf2": { "$ref": "#/$defs/values~1leaf" },
            "leaf3": { "$ref": "#/$defs/values~1leaf", "description": "A sibling annotation." },
            "data": { "examples": [{ "$ref": "#/$defs/values~1leaf" }], "const": "#/$defs/values~1leaf" }
        }
    })
}

#[test]
fn shortening_is_a_bijection_that_inverts_to_the_readable_document() -> eyre::Result<()> {
    let readable = readable_document();
    let shortened = shorten_definition_names(&readable);

    // Total coverage, unique destinations, equal definition counts.
    sim_assert_eq!(
        have: shortened.readable_names.values().cloned().collect::<BTreeSet<_>>(),
        want: definitions(&readable).into_keys().collect::<BTreeSet<_>>()
    );
    sim_assert_eq!(
        have: shortened.readable_names.len(),
        want: definitions(&readable).len()
    );
    sim_assert_eq!(
        have: definitions(&shortened.schema).len(),
        want: definitions(&readable).len()
    );

    // The most referenced definition gets the shortest name.
    sim_assert_eq!(
        have: &shortened.schema,
        want: &json!({
            "$defs": {
                "1": { "properties": { "name": { "type": "string" } } },
                "2": { "properties": { "next": { "$ref": "#/$defs/4" } } },
                "3": { "items": { "$ref": "#/$defs/3" }, "type": "array" },
                "4": {
                    "properties": { "back": { "$ref": "#/$defs/2" } },
                    "default": { "$ref": "#/$defs/values~1mutual" }
                }
            },
            "properties": {
                "cycle": { "$ref": "#/$defs/3" },
                "mutual": { "$ref": "#/$defs/2" },
                "leaf": { "$ref": "#/$defs/1/properties/name" },
                "leaf2": { "$ref": "#/$defs/1" },
                "leaf3": { "$ref": "#/$defs/1", "description": "A sibling annotation." },
                "data": { "examples": [{ "$ref": "#/$defs/values~1leaf" }], "const": "#/$defs/values~1leaf" }
            }
        })
    );

    // Every rewritten reference resolves in the shortened document.
    for pointer in [
        "/properties/cycle",
        "/properties/mutual",
        "/properties/leaf",
        "/properties/leaf3",
        "/$defs/3/items",
        "/$defs/2/properties/next",
        "/$defs/4/properties/back",
    ] {
        let reference =
            reference_at(&shortened.schema, pointer).ok_or_eyre("reference expected")?;
        let target = reference
            .strip_prefix('#')
            .ok_or_eyre("local reference expected")?;
        assert!(
            shortened.schema.pointer(target).is_some(),
            "{reference} dangles"
        );
    }

    let mut restored = shortened.schema;
    rename_definitions(&mut restored, &shortened.readable_names);
    sim_assert_eq!(have: restored, want: readable);
    Ok(())
}

#[test]
fn shortening_abstains_on_references_it_cannot_rewrite() {
    let external = json!({
        "$defs": { "values/leaf": { "type": "string" } },
        "properties": { "a": { "$ref": "other.json#/$defs/values~1leaf" } }
    });
    let encoded = json!({
        "$defs": { "values/leaf": { "type": "string" } },
        "properties": { "a": { "$ref": "#/%24defs/values~1leaf" } }
    });
    let dangling = json!({
        "$defs": { "values/leaf": { "type": "string" } },
        "properties": { "a": { "$ref": "#/$defs/1" } }
    });
    // `$dynamicRef` to a JSON Pointer behaves like `$ref`, which the rename
    // would leave dangling.
    let dynamic = json!({
        "$defs": { "values/long-name": { "type": "string" } },
        "properties": { "a": { "$dynamicRef": "#/$defs/values~1long-name" } }
    });
    // A fragment-only `$id` names a location, not a new resource, so the
    // reference below it still resolves against the root.
    let fragment_identifier = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {},
        "$defs": { "values/long-name": { "type": "string" } },
        "properties": {
            "a": { "$id": "#entry", "properties": { "b": { "$ref": "#/$defs/values~1long-name" } } }
        }
    });
    let nested_resource = json!({
        "$defs": { "values/leaf": { "type": "string" } },
        "properties": {
            "a": { "$ref": "#/$defs/values~1leaf" },
            "b": { "$id": "nested.json", "$defs": { "values/leaf": {} }, "$ref": "#/$defs/values~1leaf" }
        }
    });
    let anchor = json!({
        "$defs": { "values/long-name": { "$anchor": "leaf", "type": "string" } },
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
        let shortened = shorten_definition_names(&schema);
        sim_assert_eq!(have: shortened.readable_names, want: BTreeMap::new());
        sim_assert_eq!(have: shortened.schema, want: schema);
    }
}

#[test]
fn helm_errors_translate_back_to_readable_names() {
    let readable_names = BTreeMap::from([
        (
            "2b".to_string(),
            "k8s/io.k8s.api.core.v1.Probe.exec".to_string(),
        ),
        ("1".to_string(), "values/web".to_string()),
    ]);
    // Recorded from `helm lint` (Helm v4.3.0) on a shortened schema.
    let helm_error = indoc::indoc! {r##"
        [ERROR] templates/: values don't meet the specifications of the schema(s) in the following chart(s):
        refchk:
        "file:///values.schema.json#/$defs/2b" is not valid against metaschema: jsonschema validation failed with 'http://json-schema.org/draft-07/schema#'
        - at '/pattern': '^(?=x)' is not valid regex: error parsing regexp: invalid or unsupported Perl syntax: `(?=`
        json-pointer in "file:///values.schema.json#/$defs/1/properties/a" not found; $defs/21 and $defs/1b stay
    "##};

    sim_assert_eq!(
        have: expand_short_definition_names(helm_error, &readable_names),
        want: indoc::indoc! {r##"
            [ERROR] templates/: values don't meet the specifications of the schema(s) in the following chart(s):
            refchk:
            "file:///values.schema.json#/$defs/k8s~1io.k8s.api.core.v1.Probe.exec" is not valid against metaschema: jsonschema validation failed with 'http://json-schema.org/draft-07/schema#'
            - at '/pattern': '^(?=x)' is not valid regex: error parsing regexp: invalid or unsupported Perl syntax: `(?=`
            json-pointer in "file:///values.schema.json#/$defs/values~1web/properties/a" not found; $defs/21 and $defs/1b stay
        "##}
        .to_string()
    );
}
