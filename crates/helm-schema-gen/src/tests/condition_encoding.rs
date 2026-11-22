use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{
    CollectionDomain, CollectionQuantifier, ConditionalGuard, Guard, GuardValue,
    RequirementConditionBuilder, ValuePattern, ValuesPath,
};
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::condition_encoding::{
    AbsenceDefaults, ConditionFragmentCache, ConditionPolarity, guard_encodes_fully,
};
use crate::schema_node::SchemaNode;
use crate::schema_tree::TypePartitionMemo;

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

fn reference_guard_encodes_fully(
    guard: &ConditionalGuard,
    ancestor: &[String],
    values: &serde_yaml::Value,
    absence: AbsenceDefaults<'_>,
) -> bool {
    match guard {
        ConditionalGuard::Decision(graph) => graph.decisions().iter().all(|node| {
            node.test.is_exact_requirement_atom()
                && reference_guard_encodes_fully(&node.test, ancestor, values, absence)
        }),
        ConditionalGuard::Not(inner) => {
            reference_guard_encodes_fully(inner, ancestor, values, absence)
        }
        ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
            !guards.is_empty()
                && guards
                    .iter()
                    .all(|guard| reference_guard_encodes_fully(guard, ancestor, values, absence))
        }
        other => !build_condition_clauses(
            std::slice::from_ref(other),
            ancestor,
            values,
            absence,
            ConditionPolarity::Widen,
        )
        .is_empty(),
    }
}

fn availability_guards() -> eyre::Result<Vec<ConditionalGuard>> {
    let mut guards = Vec::new();
    for path in [
        "",
        "subject",
        "subject.*",
        r"subject.\*",
        "dependency.subject",
    ] {
        let path = ValuesPath::parse(path);
        guards.extend([
            ConditionalGuard::Absent { path: path.clone() },
            ConditionalGuard::Truthy { path: path.clone() },
            ConditionalGuard::Eq {
                path: path.clone(),
                value: GuardValue::Null,
            },
            ConditionalGuard::IntLt {
                path: path.clone(),
                bound: 1,
            },
            ConditionalGuard::MatchesValue {
                path: path.clone(),
                pattern: ValuePattern::Array {
                    prefix: vec![ValuePattern::Any],
                    items: Some(Box::new(ValuePattern::TemplateFree)),
                },
            },
        ]);
        for pattern in ["^a$", "["] {
            guards.push(ConditionalGuard::MatchesPattern {
                path: path.clone(),
                pattern: pattern.into(),
            });
            for domain in [CollectionDomain::ObjectKeys, CollectionDomain::ObjectValues] {
                for quantifier in [CollectionQuantifier::All, CollectionQuantifier::Any] {
                    guards.push(ConditionalGuard::CollectionPattern {
                        path: path.clone(),
                        domain,
                        quantifier,
                        pattern: pattern.into(),
                    });
                }
            }
        }
    }
    let atom = helm_schema_core::RequirementAtom::Value(Guard::Truthy {
        path: ValuesPath::parse("subject"),
    });
    let atoms = [atom.clone()];
    let mut work = 16_384;
    let mut bdd = helm_schema_core::RequirementBdd::new(&atoms, &mut work)
        .ok_or_eyre("availability atom inventory exhausted")?;
    let root = bdd.atom(&atom).ok_or_eyre("availability atom exhausted")?;
    guards.push(
        bdd.finish(root)
            .ok_or_eyre("availability export exhausted")?,
    );
    let first = guards
        .first()
        .cloned()
        .ok_or_eyre("availability guard missing")?;
    guards.extend([
        ConditionalGuard::AllOf(Vec::new().into()),
        ConditionalGuard::AnyOf(Vec::new().into()),
        ConditionalGuard::Not(std::sync::Arc::new(first.clone())),
        ConditionalGuard::AllOf(vec![first.clone(), first.clone()].into()),
        ConditionalGuard::AnyOf(vec![first].into()),
    ]);
    Ok(guards)
}

#[test]
fn encodability_cache_matches_independent_checks_across_full_contexts() -> eyre::Result<()> {
    let documents = [
        serde_yaml::Value::Null,
        serde_yaml::from_str(indoc! {"
            subject: {a: alpha, '*': text}
            dependency: {subject: beta}
        "})?,
        serde_yaml::from_str(indoc! {"
            subject: {a: 7}
            dependency: {subject: [false]}
        "})?,
    ];
    let roots = [BTreeSet::new(), BTreeSet::from([vec!["dependency".into()]])];
    let ancestors = [Vec::new(), vec!["subject".into()], vec!["other".into()]];
    let guards = availability_guards()?;
    let mut cache = ConditionFragmentCache::default();
    for _ in 0..2 {
        for values in &documents {
            for runtime_defaults in &documents {
                for dependency_refill in &documents {
                    for dependency_roots in &roots {
                        let absence = AbsenceDefaults {
                            runtime_defaults,
                            dependency_refill,
                            dependency_roots,
                        };
                        for ancestor in &ancestors {
                            for guard in &guards {
                                sim_assert_eq!(
                                    have: cache.encodes_fully(guard, ancestor, values, absence),
                                    want: reference_guard_encodes_fully(guard, ancestor, values, absence)
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(cache.definitions.is_empty());
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
    cache.reserve(&serde_json::json!({ "definitions": { "helm-condition-0": true }, "allOf": [{ "$ref": "#/definitions/helm-condition-1" }], "properties": { "id": { "const": { "$id": "data" } } } }));
    let guard = ConditionalGuard::Not(std::sync::Arc::new(ConditionalGuard::Truthy {
        path: ValuesPath::parse("flag"),
    }));
    let encoded = cache.clauses(&[guard], &[], &empty, absence, ConditionPolarity::Widen);
    sim_assert_eq!(have: encoded.into_iter().map(crate::schema_node::SchemaNode::into_value).collect::<Vec<_>>(), want: vec![serde_json::json!({ "$ref": "#/definitions/helm-condition-2" })]);
    sim_assert_eq!(have: cache.definitions.keys().cloned().collect::<Vec<_>>(), want: vec!["helm-condition-2".to_owned()]);
}

#[test]
fn scalar_default_validation_resolves_only_the_supplied_condition_definitions() {
    let schema = serde_json::json!({ "if": { "$ref": "#/definitions/condition" }, "then": { "const": "yes" }, "else": { "const": "" } });
    let definitions = std::collections::BTreeMap::from([
        (
            "condition".to_owned(),
            serde_json::json!({ "$ref": "#/definitions/inner" }),
        ),
        (
            "inner".to_owned(),
            serde_json::json!({ "$ref": format!("#/definitions/{}", crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME) }),
        ),
        (
            "unreachable".to_owned(),
            serde_json::json!({ "$ref": "#/definitions/missing" }),
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
    let mut inline_cache = ConditionFragmentCache::default();
    let original = SchemaNode::all_of(inline_cache.clauses(
        &guards,
        &ancestor,
        &empty,
        absence,
        ConditionPolarity::Widen,
    ));
    let mut shared = crate::schema_tree::SchemaDocument::new_root_object(crate::RootPolicy::Open);
    let mut inline = shared.clone();
    let type_partition = cache.is_type_partition(&encoded);
    assert!(type_partition);
    shared.append_conditional_with_type_partition(
        &ancestor,
        encoded,
        crate::schema_node::SchemaNode::from_value(serde_json::json!({"required":["child"]})),
        type_partition,
    );
    let type_partition = inline_cache.is_type_partition(&original);
    assert!(type_partition);
    inline.append_conditional_with_type_partition(
        &ancestor,
        original,
        crate::schema_node::SchemaNode::from_value(serde_json::json!({"required":["child"]})),
        type_partition,
    );
    sim_assert_eq!(have: expand_conditions(shared.into_value(), &cache.definitions), want: inline.into_value());
}

#[derive(Clone, Default)]
struct RequirementVisits {
    encoded: Arc<AtomicU64>,
    available: Arc<AtomicU64>,
}

impl tracing::field::Visit for RequirementVisits {
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        match field.name() {
            "requirement_encoding_visits" => self.encoded.store(value, Ordering::Relaxed),
            "requirement_availability_visits" => self.available.store(value, Ordering::Relaxed),
            _ => {}
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for RequirementVisits {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _: &tracing::span::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attributes.metadata().name() == "condition_encoding_storage" {
            attributes.record(&mut self.clone());
        }
    }
}

fn flag_guard(name: &str) -> ConditionalGuard {
    ConditionalGuard::Eq {
        path: ValuesPath::parse(name),
        value: GuardValue::Bool(true),
    }
}

fn flag_schema(name: &str) -> Value {
    json!({"type": "object", "properties": {(name): {"enum": [true]}}, "required": [name]})
}

fn definition_ref(name: &str) -> Value {
    json!({"$ref": helm_schema_json_schema_walk::GENERATED_DEFINITIONS.reference(name)})
}

#[test]
fn requirement_encoding_and_availability_prune_shared_interiors() -> eyre::Result<()> {
    use tracing_subscriber::prelude::*;

    let width = 1_024_u64;
    let roots = 512_u64;
    let mut builder = RequirementConditionBuilder::default();
    let shared = (0..width)
        .map(|index| builder.guard(flag_guard(&format!("shared-{index:04}"))))
        .collect::<Vec<_>>();
    let shared = builder.or(shared);
    let root_ids = (0..roots)
        .map(|index| {
            let gate = builder.guard(flag_guard(&format!("gate-{index:04}")));
            builder.and([gate, shared])
        })
        .collect::<Vec<_>>();
    let arena = builder.freeze();
    let empty = serde_yaml::Value::Null;
    let dependency_roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &dependency_roots,
    };
    let mut cache = ConditionFragmentCache::new();
    let mut encoded_roots = Vec::new();
    for id in root_ids {
        let requirement = ConditionalGuard::Requirement(arena.root(id, true));
        assert!(cache.encodes_fully(&requirement, &[], &empty, absence));
        let clauses = cache.clauses(
            &[requirement],
            &[],
            &empty,
            absence,
            ConditionPolarity::Widen,
        );
        encoded_roots.extend(clauses.into_iter().map(SchemaNode::into_value));
    }

    let visits = RequirementVisits::default();
    let subscriber = tracing_subscriber::registry().with(visits.clone());
    tracing::subscriber::with_default(subscriber, || cache.record_storage());
    // After the first root, each traversal enters only the root, its gate, and the shared cache hit.
    let expected_visits = width + roots * 3;
    sim_assert_eq!(have: visits.encoded.load(Ordering::Relaxed), want: expected_visits);
    sim_assert_eq!(have: visits.available.load(Ordering::Relaxed), want: expected_visits);

    let alternatives = (0..width)
        .map(|index| flag_schema(&format!("shared-{index:04}")))
        .collect::<Vec<_>>();
    let mut definitions = BTreeMap::from([(
        "helm-condition-0".to_owned(),
        json!({"anyOf": alternatives}),
    )]);
    let mut expected_roots = Vec::new();
    for index in 0..roots {
        let key = format!("helm-condition-{}", index + 1);
        definitions.insert(key.clone(), json!({"allOf": [flag_schema(&format!("gate-{index:04}")), definition_ref("helm-condition-0")]}));
        expected_roots.push(definition_ref(&key));
    }
    let schema = json!({"definitions": cache.definitions, "anyOf": encoded_roots});
    sim_assert_eq!(have: &schema, want: &json!({"definitions": definitions, "anyOf": expected_roots}));
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&json!({"shared-0511": true, "gate-0511": true})));
    assert!(!validator.is_valid(&json!({"shared-0511": true})));
    assert!(!validator.is_valid(&json!({"gate-0511": true})));
    Ok(())
}

#[test]
fn unavailable_requirement_nodes_do_not_poison_other_roots_or_anchors() {
    let empty = serde_yaml::Value::Null;
    let dependency_roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &dependency_roots,
    };
    let mut builder = RequirementConditionBuilder::default();
    let outside = builder.guard(flag_guard("outside"));
    let inside = builder.guard(flag_guard("inside.ready"));
    let root = builder.split(flag_guard("inside.testing"), outside, inside);
    let arena = builder.freeze();
    let requirement = ConditionalGuard::Requirement(arena.root(root, true));
    let available_sibling = ConditionalGuard::Requirement(arena.root(inside, true));
    let anchor = ["inside".to_owned()];
    let mut cache = ConditionFragmentCache::new();
    assert!(!cache.encodes_fully(&requirement, &anchor, &empty, absence));
    assert!(cache.encodes_fully(&available_sibling, &anchor, &empty, absence));
    assert!(cache.encodes_fully(&requirement, &[], &empty, absence));
}

#[test]
fn type_partition_diamonds_visit_each_definition_once() {
    let mut previous = "kind/0".to_owned();
    let mut definitions = BTreeMap::from([(previous.clone(), json!({"type": "string"}))]);
    for level in 1..=24 {
        let name = format!("kind/{level}");
        let reference = definition_ref(&previous);
        definitions.insert(
            name.clone(),
            json!({"anyOf": [
                {"not": reference}, {"allOf": [reference, {"type": "array"}]}
            ]}),
        );
        previous = name;
    }
    let mut memo = TypePartitionMemo::default();
    assert!(memo.classify(&definition_ref(&previous), &definitions));
    assert!(memo.classify(&definition_ref("kind/20"), &definitions));
    sim_assert_eq!(have: memo.definition_visits(), want: definitions.len());

    let mut non_partition = "members/0".to_owned();
    definitions.insert(non_partition.clone(), json!({"required": ["child"]}));
    for level in 1..=24 {
        let name = format!("members/{level}");
        definitions.insert(
            name.clone(),
            json!({"allOf": [
                definition_ref(&non_partition), definition_ref(&previous)
            ]}),
        );
        non_partition = name;
    }
    assert!(!memo.classify(&definition_ref(&non_partition), &definitions));
    for level in 0..=24 {
        assert!(!memo.classify(&definition_ref(&format!("members/{level}")), &definitions));
    }
    sim_assert_eq!(have: memo.definition_visits(), want: definitions.len());
}

#[test]
fn type_partitions_require_closed_references_in_the_current_scope() {
    let mut definitions = BTreeMap::from([
        ("pending".to_owned(), definition_ref("late")),
        (
            "recurse".to_owned(),
            json!({"not": definition_ref("recurse")}),
        ),
    ]);
    let mut memo = TypePartitionMemo::default();
    let pending = definition_ref("pending");
    assert!(!memo.classify(&pending, &definitions));
    definitions.insert("late".to_owned(), json!({"type": "string"}));
    assert!(memo.classify(&pending, &definitions));

    for reference in [
        definition_ref("recurse"),
        json!({"$ref": "#/definitions/late/type"}),
        json!({"$ref": "other.json#/definitions/late"}),
        json!({"$id": "urn:other", "$ref": "#/definitions/late"}),
        // The other container's spelling names a different, absent definition.
        json!({"$ref": "#/$defs/late"}),
    ] {
        assert!(!memo.classify(&reference, &definitions));
    }
    let mut other_scope = TypePartitionMemo::default();
    assert!(!other_scope.classify(&pending, &BTreeMap::new()));
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

#[test]
fn requirement_decisions_test_coalesced_empty_maps_exactly() -> eyre::Result<()> {
    let atom = helm_schema_core::RequirementAtom::Value(Guard::Truthy {
        path: ValuesPath::parse("feature"),
    });
    let atoms = vec![atom.clone()];
    let mut work = 16_384;
    let mut bdd = helm_schema_core::RequirementBdd::new(&atoms, &mut work)
        .ok_or_eyre("BDD inventory exhausted")?;
    let root = bdd.atom(&atom).ok_or_eyre("BDD atom exhausted")?;
    let guard = bdd.finish(root).ok_or_eyre("BDD export exhausted")?;
    let defaults = serde_yaml::from_str("feature: {enabled: true}")?;
    let absent = serde_yaml::Value::Null;
    let dependency_roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &absent,
        dependency_refill: &absent,
        dependency_roots: &dependency_roots,
    };
    let mut cache = ConditionFragmentCache::new();
    let clauses = cache.clauses(&[guard], &[], &defaults, absence, ConditionPolarity::Widen);
    let truthy = json!({"anyOf": [{"const": true}, {"type": "number", "not": {"const": 0}}, {"type": "string", "minLength": 1}, {"type": "array", "minItems": 1}, {"type": "object", "minProperties": 1}]});
    cache.definitions.insert("t".to_owned(), truthy.clone());
    let schema = json!({"$schema": "http://json-schema.org/draft-07/schema#", "definitions": cache.definitions, "allOf": clauses.into_iter().map(crate::schema_node::SchemaNode::into_value).collect::<Vec<_>>()});
    sim_assert_eq!(have: &schema, want: &json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {"t": truthy, "helm-condition-0": {"if": {"type": "object", "properties": {"feature": {"$ref": "#/definitions/t"}}, "required": ["feature"]}, "then": true, "else": false}},
        "allOf": [{"$ref": "#/definitions/helm-condition-0"}]
    }));
    let validator = jsonschema::validator_for(&schema)?;
    for feature in [json!({}), json!([]), json!(false), json!(0), json!("")] {
        assert!(!validator.is_valid(&json!({"feature": feature})));
    }
    assert!(validator.is_valid(&json!({"feature": {"enabled": true}})));
    Ok(())
}
