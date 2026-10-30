use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{
    CollectionDomain, CollectionQuantifier, ConditionalGuard, Guard, ValuePattern, ValuesPath,
};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::condition_encoding::{
    AbsenceDefaults, ConditionFragmentCache, ConditionPolarity, guard_encodes_fully,
};

pub(super) fn build_condition_clauses(
    guards: &[ConditionalGuard],
    ancestor_segments: &[String],
    values_yaml_doc: &serde_yaml::Value,
    absence: AbsenceDefaults<'_>,
    polarity: ConditionPolarity,
) -> Vec<crate::schema_node::SchemaNode> {
    ConditionFragmentCache::default().clauses(
        guards,
        ancestor_segments,
        values_yaml_doc,
        absence,
        polarity,
    )
}

#[test]
fn condition_cache_separates_selected_values_documents() -> eyre::Result<()> {
    let base = serde_yaml::from_str("feature: {}")?;
    let guarded = serde_yaml::from_str("feature: {enabled: true}")?;
    let absent = serde_yaml::Value::Null;
    let dependency_roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &absent,
        dependency_refill: &absent,
        dependency_roots: &dependency_roots,
    };
    let guards = vec![ConditionalGuard::Truthy {
        path: ValuesPath::parse("feature"),
    }];
    let ancestor = Vec::new();
    let mut cache = ConditionFragmentCache::new();

    let base_fragments =
        cache.clauses(&guards, &ancestor, &base, absence, ConditionPolarity::Widen);
    let guarded_fragments = cache.clauses(
        &guards,
        &ancestor,
        &guarded,
        absence,
        ConditionPolarity::Widen,
    );
    let expected_guarded = build_condition_clauses(
        &guards,
        &ancestor,
        &guarded,
        absence,
        ConditionPolarity::Widen,
    );

    sim_assert_eq!(have: guarded_fragments, want: expected_guarded);
    eyre::ensure!(
        base_fragments != guarded_fragments,
        "distinct values documents must produce distinct cached fragments"
    );
    Ok(())
}

struct CollectionCase {
    all_guard: Guard,
    any_guard: Guard,
    wrong_kind: Value,
    all_schema: Value,
    any_schema: Value,
    samples: Vec<CollectionSample>,
}

struct CollectionSample {
    value: Value,
    all: bool,
    any: bool,
}

#[test]
fn collection_pattern_quantifiers_are_exact_in_both_polarities() -> eyre::Result<()> {
    for case in [
        array_collection_case(),
        object_values_collection_case(),
        object_keys_collection_case(),
    ] {
        check_collection_case(case)?;
    }
    Ok(())
}

fn array_collection_case() -> CollectionCase {
    let member = json!({"type": "string", "pattern": "^a$"});
    CollectionCase {
        all_guard: Guard::MatchesValue {
            path: ValuesPath::parse("subject"),
            pattern: ValuePattern::Array {
                prefix: vec![],
                items: Some(Box::new(ValuePattern::String {
                    pattern: "^a$".to_owned(),
                })),
            },
        },
        any_guard: Guard::MatchesValue {
            path: ValuesPath::parse("subject"),
            pattern: ValuePattern::Contains(Box::new(ValuePattern::String {
                pattern: "^a$".to_owned(),
            })),
        },
        wrong_kind: json!({}),
        all_schema: json!({"type": "array", "items": member}),
        any_schema: json!({"type": "array", "contains": member}),
        samples: vec![
            CollectionSample {
                value: json!([]),
                all: true,
                any: false,
            },
            CollectionSample {
                value: json!(["a"]),
                all: true,
                any: true,
            },
            CollectionSample {
                value: json!(["a", "b"]),
                all: false,
                any: true,
            },
            CollectionSample {
                value: json!(["b"]),
                all: false,
                any: false,
            },
            CollectionSample {
                value: json!(["a", 1]),
                all: false,
                any: true,
            },
            CollectionSample {
                value: json!([1]),
                all: false,
                any: false,
            },
        ],
    }
}

fn object_values_collection_case() -> CollectionCase {
    let member = json!({"type": "string", "pattern": "^a$"});
    CollectionCase {
        all_guard: Guard::CollectionPattern {
            path: ValuesPath::parse("subject"),
            domain: CollectionDomain::ObjectValues,
            quantifier: CollectionQuantifier::All,
            pattern: "^a$".to_owned(),
        },
        any_guard: Guard::CollectionPattern {
            path: ValuesPath::parse("subject"),
            domain: CollectionDomain::ObjectValues,
            quantifier: CollectionQuantifier::Any,
            pattern: "^a$".to_owned(),
        },
        wrong_kind: json!([]),
        all_schema: json!({"type": "object", "additionalProperties": member}),
        any_schema: json!({"type": "object", "not": {"additionalProperties": {"not": member}}}),
        samples: vec![
            CollectionSample {
                value: json!({}),
                all: true,
                any: false,
            },
            CollectionSample {
                value: json!({"x": "a"}),
                all: true,
                any: true,
            },
            CollectionSample {
                value: json!({"x": "a", "y": "b"}),
                all: false,
                any: true,
            },
            CollectionSample {
                value: json!({"x": "b"}),
                all: false,
                any: false,
            },
            CollectionSample {
                value: json!({"x": "a", "y": 1}),
                all: false,
                any: true,
            },
            CollectionSample {
                value: json!({"x": 1}),
                all: false,
                any: false,
            },
        ],
    }
}

fn object_keys_collection_case() -> CollectionCase {
    let member = json!({"type": "string", "pattern": "^a$"});
    CollectionCase {
        all_guard: Guard::CollectionPattern {
            path: ValuesPath::parse("subject"),
            domain: CollectionDomain::ObjectKeys,
            quantifier: CollectionQuantifier::All,
            pattern: "^a$".to_owned(),
        },
        any_guard: Guard::CollectionPattern {
            path: ValuesPath::parse("subject"),
            domain: CollectionDomain::ObjectKeys,
            quantifier: CollectionQuantifier::Any,
            pattern: "^a$".to_owned(),
        },
        wrong_kind: json!([]),
        all_schema: json!({"type": "object", "propertyNames": member}),
        any_schema: json!({"type": "object", "not": {"propertyNames": {"not": member}}}),
        samples: vec![
            CollectionSample {
                value: json!({}),
                all: true,
                any: false,
            },
            CollectionSample {
                value: json!({"a": 1}),
                all: true,
                any: true,
            },
            CollectionSample {
                value: json!({"a": null, "b": "a"}),
                all: false,
                any: true,
            },
            CollectionSample {
                value: json!({"b": "a"}),
                all: false,
                any: false,
            },
        ],
    }
}

fn check_collection_case(case: CollectionCase) -> eyre::Result<()> {
    let absent = serde_yaml::Value::Null;
    let dependency_roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &absent,
        dependency_refill: &absent,
        dependency_roots: &dependency_roots,
    };
    let CollectionCase {
        all_guard,
        any_guard,
        wrong_kind,
        all_schema,
        any_schema,
        samples,
    } = case;
    for (quantifier, core_guard) in [
        (CollectionQuantifier::All, all_guard),
        (CollectionQuantifier::Any, any_guard),
    ] {
        let guard = ConditionalGuard::try_from(&core_guard)
            .map_err(|()| eyre::eyre!("collection guard conversion failed"))?;
        sim_assert_eq!(have: guard.predicate(), want: helm_schema_core::Predicate::from(core_guard));
        let leaf = match quantifier {
            CollectionQuantifier::All => &all_schema,
            CollectionQuantifier::Any => &any_schema,
        };
        let expected =
            json!({"type": "object", "required": ["subject"], "properties": {"subject": leaf}});
        for polarity in [ConditionPolarity::Widen, ConditionPolarity::Narrow] {
            for negated in [false, true] {
                let guard = if negated {
                    ConditionalGuard::Not(std::sync::Arc::new(guard.clone()))
                } else {
                    guard.clone()
                };
                let fragments = build_condition_clauses(&[guard], &[], &absent, absence, polarity);
                let actual = fragments
                    .first()
                    .ok_or_eyre("missing collection condition")?
                    .clone()
                    .into_value();
                sim_assert_eq!(have: fragments.len(), want: 1);
                sim_assert_eq!(have: &actual, want: &if negated { json!({"not": expected}) } else { expected.clone() });
                let validator = jsonschema::validator_for(&actual)?;
                for CollectionSample { value, all, any } in &samples {
                    let matches = match quantifier {
                        CollectionQuantifier::All => *all,
                        CollectionQuantifier::Any => *any,
                    };
                    sim_assert_eq!(have: validator.is_valid(&json!({"subject": value})), want: matches != negated);
                }
                // Presence and collection type remain required even for a vacuous universal.
                for value in [
                    json!({}),
                    json!({"subject": null}),
                    json!({"subject": "a"}),
                    json!({"subject": false}),
                ] {
                    sim_assert_eq!(have: validator.is_valid(&value), want: negated);
                }
                sim_assert_eq!(have: validator.is_valid(&json!({"subject": wrong_kind})), want: negated);
            }
        }
    }
    Ok(())
}

#[test]
fn collection_pattern_guards_keep_their_domain_and_quantifier_when_paths_change() {
    let guard = ConditionalGuard::CollectionPattern {
        path: ValuesPath::parse("before"),
        domain: CollectionDomain::ObjectKeys,
        quantifier: CollectionQuantifier::Any,
        pattern: "^a$".to_owned(),
    };
    sim_assert_eq!(have: guard.predicate().map_value_paths(&mut |_| ValuesPath::parse("after")), want: ConditionalGuard::CollectionPattern {
        path: ValuesPath::parse("after"), domain: CollectionDomain::ObjectKeys,
        quantifier: CollectionQuantifier::Any, pattern: "^a$".to_owned(),
    }.predicate());
    sim_assert_eq!(have: guard.map_value_paths(&mut |_| ValuesPath::parse("after")), want: ConditionalGuard::CollectionPattern {
        path: ValuesPath::parse("after"), domain: CollectionDomain::ObjectKeys,
        quantifier: CollectionQuantifier::Any, pattern: "^a$".to_owned(),
    });
}

#[test]
fn collection_pattern_quantifiers_abstain_on_current_member_paths() {
    let absent = serde_yaml::Value::Null;
    let dependency_roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &absent,
        dependency_refill: &absent,
        dependency_roots: &dependency_roots,
    };
    let array_guard = ConditionalGuard::MatchesValue {
        path: ValuesPath::parse("subject.*.member"),
        pattern: ValuePattern::Array {
            prefix: vec![],
            items: Some(Box::new(ValuePattern::String {
                pattern: "^a$".to_owned(),
            })),
        },
    };
    let object_guard = ConditionalGuard::CollectionPattern {
        path: ValuesPath::parse("subject.*.member"),
        domain: CollectionDomain::ObjectValues,
        quantifier: CollectionQuantifier::All,
        pattern: "^a$".to_owned(),
    };
    for guard in [array_guard, object_guard].into_iter().flat_map(|guard| {
        [
            guard.clone(),
            ConditionalGuard::Not(std::sync::Arc::new(guard)),
        ]
    }) {
        sim_assert_eq!(have: guard_encodes_fully(&guard, &[], &absent, absence), want: false);
        for polarity in [ConditionPolarity::Widen, ConditionPolarity::Narrow] {
            sim_assert_eq!(have: build_condition_clauses(std::slice::from_ref(&guard), &[], &absent, absence, polarity), want: vec![]);
        }
    }
}

struct ValuePatternSample {
    value: Value,
    expected: bool,
}

#[test]
fn value_patterns_preserve_order_and_nested_arrays() -> eyre::Result<()> {
    let bad = ValuePattern::String {
        pattern: "^bad$".to_owned(),
    };
    let pattern = ValuePattern::Array {
        prefix: vec![
            ValuePattern::String {
                pattern: "^safe$".to_owned(),
            },
            ValuePattern::AnyOf(vec![
                bad.clone(),
                ValuePattern::Array {
                    prefix: vec![bad],
                    items: None,
                },
            ]),
        ],
        items: None,
    };
    let bad = string_pattern_schema("^bad$");
    check_value_pattern(
        &pattern,
        &json!({
            "type": "array", "minItems": 2, "additionalItems": false,
            "items": [
                string_pattern_schema("^safe$"),
                {"anyOf": [bad, {"type": "array", "minItems": 1, "items": [bad], "additionalItems": false}]},
            ],
        }),
        &[
            ValuePatternSample {
                value: json!(["safe", "bad"]),
                expected: true,
            },
            ValuePatternSample {
                value: json!(["bad", "safe"]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["safe", ["bad"]]),
                expected: true,
            },
            ValuePatternSample {
                value: json!([["bad"], "safe"]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["safe"]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["safe", "bad", "extra"]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["safe", ["bad", "extra"]]),
                expected: false,
            },
            ValuePatternSample {
                value: json!([null, "bad"]),
                expected: false,
            },
        ],
    )
}

#[test]
fn value_patterns_test_remaining_items_after_the_required_prefix() -> eyre::Result<()> {
    let pattern = ValuePattern::Array {
        prefix: vec![ValuePattern::String {
            pattern: "^first$".to_owned(),
        }],
        items: Some(Box::new(ValuePattern::AnyOf(vec![
            ValuePattern::String {
                pattern: "^tail$".to_owned(),
            },
            ValuePattern::Array {
                prefix: vec![],
                items: None,
            },
        ]))),
    };
    check_value_pattern(
        &pattern,
        &json!({
            "type": "array", "minItems": 1,
            "items": [string_pattern_schema("^first$")],
            "additionalItems": {"anyOf": [
                string_pattern_schema("^tail$"), {"type": "array", "maxItems": 0},
            ]},
        }),
        &[
            ValuePatternSample {
                value: json!(["first"]),
                expected: true,
            },
            ValuePatternSample {
                value: json!(["first", "tail", []]),
                expected: true,
            },
            ValuePatternSample {
                value: json!(["first", ["tail"]]),
                expected: false,
            },
            ValuePatternSample {
                value: json!([]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["tail", "first"]),
                expected: false,
            },
            ValuePatternSample {
                value: json!([1]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["first", false]),
                expected: false,
            },
        ],
    )
}

#[test]
fn value_patterns_empty_prefix_and_empty_union_use_valid_draft07_schemas() -> eyre::Result<()> {
    check_value_pattern(
        &ValuePattern::Array {
            prefix: vec![],
            items: None,
        },
        &json!({"type": "array", "maxItems": 0}),
        &[
            ValuePatternSample {
                value: json!([]),
                expected: true,
            },
            ValuePatternSample {
                value: json!(["x"]),
                expected: false,
            },
        ],
    )?;
    check_value_pattern(
        &ValuePattern::Array {
            prefix: vec![],
            items: Some(Box::new(ValuePattern::Any)),
        },
        &json!({"type": "array", "items": true}),
        &[
            ValuePatternSample {
                value: json!([]),
                expected: true,
            },
            ValuePatternSample {
                value: json!([null, {}, false]),
                expected: true,
            },
        ],
    )?;
    check_value_pattern(
        &ValuePattern::AnyOf(vec![]),
        &json!(false),
        &[
            ValuePatternSample {
                value: json!([]),
                expected: false,
            },
            ValuePatternSample {
                value: json!([null]),
                expected: false,
            },
        ],
    )
}

fn check_value_pattern(
    pattern: &ValuePattern,
    leaf: &Value,
    samples: &[ValuePatternSample],
) -> eyre::Result<()> {
    let guard = ConditionalGuard::MatchesValue {
        path: ValuesPath::parse("subject"),
        pattern: pattern.clone(),
    };
    sim_assert_eq!(have: ConditionalGuard::try_from(&Guard::MatchesValue { path: ValuesPath::parse("subject"), pattern: pattern.clone() }), want: Ok(guard.clone()));
    let absent = serde_yaml::Value::Null;
    let dependency_roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &absent,
        dependency_refill: &absent,
        dependency_roots: &dependency_roots,
    };
    let expected =
        &json!({"type": "object", "required": ["subject"], "properties": {"subject": leaf}});
    for polarity in [ConditionPolarity::Widen, ConditionPolarity::Narrow] {
        for negated in [false, true] {
            let guard = if negated {
                ConditionalGuard::Not(std::sync::Arc::new(guard.clone()))
            } else {
                guard.clone()
            };
            let fragments = build_condition_clauses(&[guard], &[], &absent, absence, polarity);
            let actual = fragments
                .first()
                .ok_or_eyre("missing structural value condition")?
                .clone()
                .into_value();
            sim_assert_eq!(have: fragments.len(), want: 1);
            sim_assert_eq!(have: &actual, want: &if negated { json!({"not": expected}) } else { expected.clone() });
            let validator = jsonschema::options()
                .with_draft(jsonschema::Draft::Draft7)
                .build(&actual)?;
            for ValuePatternSample { value, expected } in samples {
                sim_assert_eq!(have: pattern.matches(value), want: Some(*expected));
                sim_assert_eq!(have: validator.is_valid(&json!({"subject": value})), want: *expected != negated);
            }
            sim_assert_eq!(have: validator.is_valid(&json!({})), want: negated);
        }
    }
    Ok(())
}

fn string_pattern_schema(pattern: &str) -> Value {
    json!({"type": "string", "pattern": pattern})
}

#[test]
fn value_patterns_end_anchors_reject_trailing_line_breaks() -> eyre::Result<()> {
    let mut samples = vec![
        ValuePatternSample {
            value: json!("ab"),
            expected: true,
        },
        ValuePatternSample {
            value: json!("abx"),
            expected: false,
        },
        ValuePatternSample {
            value: json!(null),
            expected: false,
        },
    ];
    for line_break in ['\r', '\n', '\u{85}', '\u{2028}', '\u{2029}'] {
        samples.push(ValuePatternSample {
            value: json!(format!("ab{line_break}")),
            expected: false,
        });
    }
    check_value_pattern(
        &ValuePattern::String {
            pattern: "^ab$".to_owned(),
        },
        &string_pattern_schema("^ab$"),
        &samples,
    )
}

#[test]
fn value_patterns_strings_accept_explicit_line_breaks() -> eyre::Result<()> {
    check_value_pattern(
        &ValuePattern::String {
            pattern: r"^a\nb$".to_owned(),
        },
        &string_pattern_schema(r"^a\nb$"),
        &[
            ValuePatternSample {
                value: json!("a\nb"),
                expected: true,
            },
            ValuePatternSample {
                value: json!("ab"),
                expected: false,
            },
            ValuePatternSample {
                value: json!("a\nb\n"),
                expected: false,
            },
        ],
    )
}

#[test]
fn value_patterns_combine_homogeneous_items_with_a_matching_witness() -> eyre::Result<()> {
    let allowed = ValuePattern::String {
        pattern: "^[ab]$".to_owned(),
    };
    let bad = ValuePattern::String {
        pattern: "^b$".to_owned(),
    };
    let pattern = ValuePattern::AllOf(vec![
        ValuePattern::Array {
            prefix: vec![],
            items: Some(Box::new(allowed)),
        },
        ValuePattern::Contains(Box::new(bad)),
    ]);
    check_value_pattern(
        &pattern,
        &json!({"allOf": [
            {"type": "array", "items": {"type": "string", "pattern": "^[ab]$"}},
            {"type": "array", "contains": {"type": "string", "pattern": "^b$"}},
        ]}),
        &[
            ValuePatternSample {
                value: json!([]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["a"]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["b"]),
                expected: true,
            },
            ValuePatternSample {
                value: json!(["a", "b"]),
                expected: true,
            },
            ValuePatternSample {
                value: json!(["b", "a"]),
                expected: true,
            },
            ValuePatternSample {
                value: json!(["a", "c", "b"]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["b", 1]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(null),
                expected: false,
            },
            ValuePatternSample {
                value: json!({"b": true}),
                expected: false,
            },
        ],
    )
}

#[test]
fn value_patterns_contains_matches_nested_tuples() -> eyre::Result<()> {
    let pattern = ValuePattern::Contains(Box::new(ValuePattern::Array {
        prefix: vec![ValuePattern::String {
            pattern: "^good$".to_owned(),
        }],
        items: None,
    }));
    check_value_pattern(
        &pattern,
        &json!({"type": "array", "contains": {
            "type": "array", "minItems": 1,
            "items": [{"type": "string", "pattern": "^good$"}], "additionalItems": false,
        }}),
        &[
            ValuePatternSample {
                value: json!([]),
                expected: false,
            },
            ValuePatternSample {
                value: json!(["good"]),
                expected: false,
            },
            ValuePatternSample {
                value: json!([["good"]]),
                expected: true,
            },
            ValuePatternSample {
                value: json!([false, ["good"]]),
                expected: true,
            },
            ValuePatternSample {
                value: json!([["good", "extra"]]),
                expected: false,
            },
            ValuePatternSample {
                value: json!("good"),
                expected: false,
            },
        ],
    )
}

#[test]
fn value_patterns_conjunction_and_contains_keep_their_empty_set_semantics() -> eyre::Result<()> {
    check_value_pattern(
        &ValuePattern::AllOf(vec![]),
        &json!(true),
        &[
            ValuePatternSample {
                value: json!(null),
                expected: true,
            },
            ValuePatternSample {
                value: json!(false),
                expected: true,
            },
        ],
    )?;
    check_value_pattern(
        &ValuePattern::Contains(Box::new(ValuePattern::Any)),
        &json!({"type": "array", "contains": true}),
        &[
            ValuePatternSample {
                value: json!([]),
                expected: false,
            },
            ValuePatternSample {
                value: json!([null]),
                expected: true,
            },
            ValuePatternSample {
                value: json!([false, {}]),
                expected: true,
            },
            ValuePatternSample {
                value: json!({}),
                expected: false,
            },
        ],
    )
}

pub(super) fn expand_conditions(
    value: serde_json::Value,
    definitions: &std::collections::BTreeMap<String, serde_json::Value>,
) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => {
            if object.len() == 1
                && let Some(reference) = object.get("$ref").and_then(serde_json::Value::as_str)
                && let Some(reference) =
                    helm_schema_json_schema_walk::parse_definition_reference(reference)
                && let Some(body) = definitions.get(&reference.name)
            {
                return expand_conditions(body.clone(), definitions);
            }
            serde_json::Value::Object(
                object
                    .into_iter()
                    .map(|(key, child)| (key, expand_conditions(child, definitions)))
                    .collect(),
            )
        }
        serde_json::Value::Array(children) => serde_json::Value::Array(
            children
                .into_iter()
                .map(|child| expand_conditions(child, definitions))
                .collect(),
        ),
        leaf => leaf,
    }
}

#[test]
fn shared_condition_definitions_preserve_formula_scope_values_and_polarity() -> eyre::Result<()> {
    let empty = serde_yaml::Value::Null;
    let mapping = serde_yaml::from_str("feature: {enabled: true}")?;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &roots,
    };
    let leaf = ConditionalGuard::Truthy {
        path: ValuesPath::parse("feature"),
    };
    let guard = ConditionalGuard::Not(std::sync::Arc::new(ConditionalGuard::AnyOf(
        vec![leaf.clone(), leaf].into(),
    )));
    let mut cache = ConditionFragmentCache::new();
    for values in [&empty, &mapping] {
        for ancestor in [Vec::new(), vec!["feature".to_owned()]] {
            for polarity in [ConditionPolarity::Widen, ConditionPolarity::Narrow] {
                let guards = std::slice::from_ref(&guard);
                let encoded = cache.clauses(guards, &ancestor, values, absence, polarity);
                let expected =
                    build_condition_clauses(guards, &ancestor, values, absence, polarity);
                let actual = encoded
                    .into_iter()
                    .map(crate::schema_node::SchemaNode::into_value)
                    .map(|value| expand_conditions(value, &cache.definitions))
                    .collect::<Vec<_>>();
                sim_assert_eq!(have: actual, want: expected.into_iter().map(crate::schema_node::SchemaNode::into_value).collect::<Vec<_>>());
            }
        }
    }
    Ok(())
}

#[test]
fn generated_condition_graph_keeps_shared_boolean_bodies_linear() {
    let empty = serde_yaml::Value::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &roots,
    };
    let mut guard = ConditionalGuard::Truthy {
        path: ValuesPath::parse("seed"),
    };
    for index in 0..6 {
        guard = ConditionalGuard::AnyOf(
            vec![
                ConditionalGuard::AllOf(
                    vec![
                        guard.clone(),
                        ConditionalGuard::Truthy {
                            path: ValuesPath::parse(&format!("left{index}")),
                        },
                    ]
                    .into(),
                ),
                ConditionalGuard::AllOf(
                    vec![
                        ConditionalGuard::Not(std::sync::Arc::new(guard)),
                        ConditionalGuard::Truthy {
                            path: ValuesPath::parse(&format!("right{index}")),
                        },
                    ]
                    .into(),
                ),
            ]
            .into(),
        );
    }
    let mut cache = ConditionFragmentCache::new();
    let encoded = cache.clauses(
        std::slice::from_ref(&guard),
        &[],
        &empty,
        absence,
        ConditionPolarity::Widen,
    );
    let expected =
        build_condition_clauses(&[guard], &[], &empty, absence, ConditionPolarity::Widen);
    sim_assert_eq!(have: encoded.into_iter().map(crate::schema_node::SchemaNode::into_value).map(|value| expand_conditions(value, &cache.definitions)).collect::<Vec<_>>(), want: expected.into_iter().map(crate::schema_node::SchemaNode::into_value).collect::<Vec<_>>());
    assert!(cache.definitions.len() <= 48);
}

#[test]
fn private_condition_names_avoid_definitions_and_dangling_references() {
    let empty = serde_yaml::Value::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &roots,
    };
    let mut cache = ConditionFragmentCache::new();
    cache.reserve(&serde_json::json!({ "$defs": { "helm-condition-0": true }, "allOf": [{ "$ref": "#/$defs/helm-condition-1" }], "properties": { "id": { "const": { "$id": "data" } } } }));
    let guard = ConditionalGuard::Not(std::sync::Arc::new(ConditionalGuard::Truthy {
        path: ValuesPath::parse("flag"),
    }));
    let encoded = cache.clauses(&[guard], &[], &empty, absence, ConditionPolarity::Widen);
    sim_assert_eq!(have: encoded.into_iter().map(crate::schema_node::SchemaNode::into_value).collect::<Vec<_>>(), want: vec![serde_json::json!({ "$ref": "#/$defs/helm-condition-2" })]);
    sim_assert_eq!(have: cache.definitions.keys().cloned().collect::<Vec<_>>(), want: vec!["helm-condition-2".to_owned()]);
}

#[test]
fn scalar_default_validation_resolves_only_the_supplied_condition_definitions() {
    let schema = serde_json::json!({ "if": { "$ref": "#/$defs/condition" }, "then": { "const": "yes" }, "else": { "const": "" } });
    let definitions = std::collections::BTreeMap::from([
        (
            "condition".to_owned(),
            serde_json::json!({ "$ref": "#/$defs/inner" }),
        ),
        (
            "inner".to_owned(),
            serde_json::json!({ "$ref": format!("#/$defs/{}", crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME) }),
        ),
        (
            "unreachable".to_owned(),
            serde_json::json!({ "$ref": "#/$defs/missing" }),
        ),
    ]);
    let declared = serde_json::json!("yes");
    sim_assert_eq!(have: crate::resolve_policy::preserve_declared_default_with_definitions(schema.clone(), &declared, &definitions), want: schema.clone());
    let fallback = super::preserve_declared_default_in_schema(schema.clone(), &declared);
    sim_assert_eq!(have: fallback, want: serde_json::json!({ "anyOf": [{ "const": "yes" }, schema] }));
}

#[test]
fn shared_type_partition_conditions_keep_the_original_host_shape() {
    let empty = serde_yaml::Value::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &roots,
    };
    let guards = [ConditionalGuard::Not(std::sync::Arc::new(
        ConditionalGuard::AnyOf(
            vec![
                ConditionalGuard::TypeIs {
                    path: ValuesPath::parse("value"),
                    schema_type: "string".to_owned(),
                },
                ConditionalGuard::TypeIs {
                    path: ValuesPath::parse("value"),
                    schema_type: "array".to_owned(),
                },
            ]
            .into(),
        ),
    ))];
    let ancestor = vec!["value".to_owned()];
    let mut cache = ConditionFragmentCache::new();
    let encoded = crate::schema_node::SchemaNode::all_of(cache.clauses(
        &guards,
        &ancestor,
        &empty,
        absence,
        ConditionPolarity::Widen,
    ));
    let original = crate::schema_node::SchemaNode::all_of(build_condition_clauses(
        &guards,
        &ancestor,
        &empty,
        absence,
        ConditionPolarity::Widen,
    ));
    let mut shared = crate::schema_tree::SchemaDocument::new_root_object(crate::RootPolicy::Open);
    let mut inline = shared.clone();
    shared.append_conditional_with_definitions(
        &ancestor,
        encoded,
        crate::schema_node::SchemaNode::from_value(serde_json::json!({"required":["child"]})),
        &cache.definitions,
    );
    inline.append_conditional_with_definitions(
        &ancestor,
        original,
        crate::schema_node::SchemaNode::from_value(serde_json::json!({"required":["child"]})),
        &std::collections::BTreeMap::new(),
    );
    sim_assert_eq!(have: expand_conditions(shared.into_value(), &cache.definitions), want: inline.into_value());
}

#[test]
fn resource_scopes_and_addressed_inline_conditions_keep_bodies_inline() {
    let empty = serde_yaml::Value::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &roots,
    };
    let guard = ConditionalGuard::Not(std::sync::Arc::new(ConditionalGuard::Truthy {
        path: ValuesPath::parse("flag"),
    }));
    for source in [
        serde_json::json!({ "properties": { "nested": { "$id": "urn:example:nested", "type": "object" } } }),
        serde_json::json!({ "$ref": "#/allOf/0/if/allOf/0" }),
    ] {
        let mut cache = ConditionFragmentCache::new();
        cache.reserve(&source);
        let encoded = cache.clauses(
            std::slice::from_ref(&guard),
            &[],
            &empty,
            absence,
            ConditionPolarity::Widen,
        );
        let expected = build_condition_clauses(
            std::slice::from_ref(&guard),
            &[],
            &empty,
            absence,
            ConditionPolarity::Widen,
        );
        sim_assert_eq!(have: encoded.into_iter().map(crate::schema_node::SchemaNode::into_value).collect::<Vec<_>>(), want: expected.into_iter().map(crate::schema_node::SchemaNode::into_value).collect::<Vec<_>>());
        sim_assert_eq!(have: cache.definitions.is_empty(), want: true);
    }
}

/// Relativizing an absent source never turns its arm into an unconditional provider constraint.
#[test]
fn current_value_absence_remains_exact_in_both_polarities() -> eyre::Result<()> {
    let defaults = serde_yaml::Value::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &defaults,
        dependency_refill: &defaults,
        dependency_roots: &roots,
    };
    let root = ConditionalGuard::Absent {
        path: ValuesPath::default(),
    };
    assert!(!guard_encodes_fully(&root, &[], &defaults, absence));
    let guard = ConditionalGuard::Absent {
        path: ValuesPath::parse("subject"),
    };
    for polarity in [ConditionPolarity::Widen, ConditionPolarity::Narrow] {
        for negated in [false, true] {
            let guard = if negated {
                ConditionalGuard::Not(std::sync::Arc::new(guard.clone()))
            } else {
                guard.clone()
            };
            assert!(guard_encodes_fully(
                &guard,
                &["subject".to_owned()],
                &defaults,
                absence
            ));
            let fragments = build_condition_clauses(
                &[guard],
                &["subject".to_owned()],
                &defaults,
                absence,
                polarity,
            );
            let actual = fragments
                .first()
                .ok_or_eyre("missing current-value absence condition")?
                .clone()
                .into_value();
            sim_assert_eq!(have: fragments.len(), want: 1);
            let expected = json!({"enum": [null]});
            sim_assert_eq!(have: &actual, want: &if negated { json!({"not": expected}) } else { expected });
        }
    }
    Ok(())
}
