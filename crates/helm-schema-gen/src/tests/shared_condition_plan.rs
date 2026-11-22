use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use color_eyre::eyre;
use helm_schema_core::{ConditionalGuard, GuardConjunction, GuardPrefix, GuardValue, ValuesPath};
use serde_json::{Value, json};
use serde_yaml::Value as YamlValue;
use test_util::prelude::sim_assert_eq;

use crate::RootPolicy;
use crate::condition_encoding::{
    AbsenceDefaults, ConditionFragmentCache, ConditionPolarity, EncodedConditionPlan,
};
use crate::provider_definitions::insert_definitions_into_root;
use crate::schema_node::SchemaNode;
use crate::schema_tree::{SchemaDocument, draft07_root_document};
use crate::uncoalesced_root::UncoalescedRootGate;

fn equals(path: &str, value: &str) -> ConditionalGuard {
    ConditionalGuard::Eq {
        path: ValuesPath::parse(path),
        value: GuardValue::String(value.to_owned()),
    }
}

fn not(guard: ConditionalGuard) -> ConditionalGuard {
    ConditionalGuard::Not(Arc::new(guard))
}

fn test_absence<'a>(
    values: &'a YamlValue,
    dependency_roots: &'a BTreeSet<Vec<String>>,
) -> AbsenceDefaults<'a> {
    AbsenceDefaults {
        runtime_defaults: values,
        dependency_refill: values,
        dependency_roots,
    }
}

#[test]
fn plan_identity_matches_legacy_json_across_collapsed_junctions() {
    let a = equals("a", "yes");
    let b = equals("b", "yes");
    let a_row = GuardConjunction::from(vec![a.clone()]);
    let b_row = GuardConjunction::from(vec![b.clone()]);
    let cases = [
        Vec::new(),
        vec![GuardConjunction::default()],
        vec![a_row.clone()],
        vec![b_row.clone()],
        vec![a_row.clone(), b_row.clone()],
        vec![b_row, a_row],
        vec![GuardConjunction::from(vec![ConditionalGuard::AnyOf(
            vec![a.clone(), b.clone()].into(),
        )])],
        vec![GuardConjunction::from(vec![a.clone(), b.clone()])],
        vec![GuardConjunction::from(vec![ConditionalGuard::AllOf(
            vec![a, b].into(),
        )])],
    ];
    let values = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = test_absence(&values, &roots);
    let mut cache = ConditionFragmentCache::default();
    let mut flat = ConditionFragmentCache::default();
    let mut keys = Vec::new();
    for rows in cases {
        let plan = EncodedConditionPlan::any_of(
            rows.iter()
                .map(|row| {
                    cache.planned_conjunction(row, &[], &values, absence, ConditionPolarity::Exact)
                })
                .collect(),
        );
        let expected = SchemaNode::any_of(
            rows.iter()
                .map(|row| {
                    SchemaNode::all_of(flat.clauses(
                        &row.to_vec(),
                        &[],
                        &values,
                        absence,
                        ConditionPolarity::Exact,
                    ))
                })
                .collect(),
        )
        .into_value();
        sim_assert_eq!(have: &plan.flat_schema().into_value(), want: &expected);
        keys.push((
            plan,
            helm_schema_json_schema_walk::canonical_json_string(&expected),
        ));
    }
    for (left, left_json) in &keys {
        for (right, right_json) in &keys {
            let expected = left_json
                .len()
                .cmp(&right_json.len())
                .then_with(|| left_json.cmp(right_json));
            sim_assert_eq!(have: left.cmp(right), want: expected);
            sim_assert_eq!(have: left == right, want: left_json == right_json);
        }
    }
}

#[test]
fn compact_prefixes_keep_the_flat_verdicts_and_leaf_definition_ordinals() -> eyre::Result<()> {
    let prefix_a = GuardPrefix::new(vec![equals("tier", "one"), not(equals("mode", "off"))]);
    let copy_a = GuardPrefix::new(prefix_a.as_slice().to_vec());
    let prefix_b = GuardPrefix::new(vec![not(equals("mode", "skip")), equals("tier", "two")]);
    let red = equals("color", "red");
    let blue = equals("color", "blue");
    let rows = [
        GuardConjunction::prefixed(vec![red.clone()], prefix_a.clone(), Vec::new()),
        GuardConjunction::prefixed(vec![equals("tone", "quiet")], prefix_b.clone(), Vec::new()),
        GuardConjunction::prefixed(vec![red.clone()], copy_a, Vec::new()),
        GuardConjunction::prefixed(vec![blue.clone()], prefix_a, Vec::new()),
        GuardConjunction::prefixed(Vec::new(), prefix_b, Vec::new()),
    ]
    .map(GuardConjunction::canonicalized);
    let values = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = test_absence(&values, &roots);
    let polarity = ConditionPolarity::Widen;
    let mut compact = ConditionFragmentCache::new();
    let mut flat = ConditionFragmentCache::new();
    let reserved = json!({ "definitions": { "helm-condition-prefix-0": {} } });
    compact.reserve(&reserved);
    flat.reserve(&reserved);
    let plan = EncodedConditionPlan::any_of(
        rows.iter()
            .map(|row| compact.planned_conjunction(row, &[], &values, absence, polarity))
            .collect(),
    );
    let flat_condition = SchemaNode::any_of(
        rows.iter()
            .map(|row| {
                SchemaNode::all_of(flat.clauses(&row.to_vec(), &[], &values, absence, polarity))
            })
            .collect(),
    )
    .into_value();
    sim_assert_eq!(have: &plan.flat_schema().into_value(), want: &flat_condition);
    sim_assert_eq!(have: &compact.definitions, want: &flat.definitions);

    let compact_condition = compact.materialize_condition(&plan).into_value();
    let colors = [red, blue].map(|guard| {
        SchemaNode::all_of(flat.clauses(&[guard], &[], &values, absence, polarity)).into_value()
    });
    sim_assert_eq!(have: &compact_condition, want: &json!({
        "anyOf": [
            {"allOf": [{"$ref": "#/definitions/helm-condition-prefix-1"}, {"anyOf": colors}]},
            {"$ref": "#/definitions/helm-condition-prefix-2"},
        ],
    }));
    assert!(!compact.definitions.contains_key("helm-condition-prefix-0"));

    let late = [not(equals("late", "off"))];
    let expected = flat.clauses(&late, &[], &values, absence, polarity);
    let actual = compact.clauses(&late, &[], &values, absence, polarity);
    sim_assert_eq!(have: actual, want: expected);
    let compact_leaves = compact
        .definitions
        .iter()
        .filter(|(name, _)| !name.starts_with("helm-condition-prefix-"))
        .map(|(name, body)| (name.clone(), body.clone()))
        .collect::<BTreeMap<_, _>>();
    sim_assert_eq!(have: compact_leaves, want: flat.definitions.clone());

    let validator = |condition: Value, definitions: &BTreeMap<String, Value>| {
        jsonschema::validator_for(&json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "definitions": definitions,
            "allOf": [condition],
        }))
    };
    let compact_validator = validator(compact_condition, &compact.definitions)?;
    let flat_validator = validator(flat_condition, &flat.definitions)?;
    for document in [
        json!({"tier": "one", "mode": "on", "color": "red"}),
        json!({"tier": "one", "mode": "on", "color": "blue"}),
        json!({"tier": "one", "mode": "on", "color": "green"}),
        json!({"tier": "one", "mode": "off", "color": "red"}),
        json!({"tier": "two", "mode": "on"}),
        json!({"tier": "two", "mode": "skip", "tone": "quiet"}),
        json!({}),
    ] {
        sim_assert_eq!(have: compact_validator.is_valid(&document), want: flat_validator.is_valid(&document));
    }
    Ok(())
}

#[test]
fn compact_conditions_keep_the_legacy_object_carrier_when_a_type_prefix_absorbs_an_arm()
-> eyre::Result<()> {
    let ancestor = vec!["kid".to_owned()];
    let host = ValuesPath::parse("kid");
    let prefix = GuardPrefix::new(vec![
        ConditionalGuard::TypeIs {
            path: host.clone(),
            schema_type: "object".to_owned(),
        },
        not(ConditionalGuard::TypeIs {
            path: host,
            schema_type: "null".to_owned(),
        }),
    ]);
    let rows = [
        GuardConjunction::prefixed(Vec::new(), prefix.clone(), Vec::new()),
        GuardConjunction::prefixed(vec![equals("kid.mode", "strict")], prefix, Vec::new()),
    ]
    .map(GuardConjunction::canonicalized);
    let values = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = test_absence(&values, &roots);
    let coalesced = json!({});
    let definitions = BTreeMap::new();
    let mut legacy_gate = UncoalescedRootGate::new(None, &coalesced, &definitions);
    let mut compact_gate = UncoalescedRootGate::new(None, &coalesced, &definitions);
    let polarity = ConditionPolarity::Widen;
    let legacy_condition = SchemaNode::any_of(
        rows.iter()
            .map(|row| {
                SchemaNode::all_of(legacy_gate.condition_clauses(
                    &row.to_vec(),
                    &ancestor,
                    &values,
                    absence,
                    polarity,
                ))
            })
            .collect(),
    );
    let plan = EncodedConditionPlan::any_of(
        rows.iter()
            .map(|row| compact_gate.planned_conjunction(row, &ancestor, &values, absence, polarity))
            .collect(),
    );
    let mut legacy = SchemaDocument::new_root_object(RootPolicy::Open);
    // A tree-owned empty slot lets the condition decide the carrier's type.
    sim_assert_eq!(have: legacy.insert_path_schema(&ancestor, SchemaNode::empty()), want: 0);
    let mut compact = legacy.clone();
    let then_schema = SchemaNode::foreign(json!({"required": ["name"]}));
    legacy_gate.append_conditional(
        &mut legacy,
        &ancestor,
        legacy_condition,
        then_schema.clone(),
    );
    compact_gate.append_planned_conditional(&mut compact, &ancestor, &plan, then_schema);
    let mut legacy = draft07_root_document(legacy.into_value(), RootPolicy::Open);
    let mut compact = draft07_root_document(compact.into_value(), RootPolicy::Open);
    insert_definitions_into_root(&mut legacy, legacy_gate.take_condition_definitions());
    insert_definitions_into_root(&mut compact, compact_gate.take_condition_definitions());

    let first = json!({"allOf": [{"type": "object"}, {"$ref": "#/definitions/helm-condition-0"}]});
    let second = json!({"allOf": [
        {"type": "object", "required": ["mode"], "properties": {"mode": {"enum": ["strict"]}}},
        {"type": "object"},
        {"$ref": "#/definitions/helm-condition-0"},
    ]});
    let not_null = json!({"not": {"type": "null"}});
    sim_assert_eq!(
        have: &legacy,
        want: &kid_object_carrier_schema(&json!({"anyOf": [&first, second]}), &json!({"helm-condition-0": &not_null})),
    );
    sim_assert_eq!(
        have: &compact,
        want: &kid_object_carrier_schema(&json!({"$ref": "#/definitions/helm-condition-prefix-0"}), &json!({
            "helm-condition-0": not_null,
            "helm-condition-prefix-0": first,
        })),
    );
    let flat_validator = jsonschema::validator_for(&legacy)?;
    let compact_validator = jsonschema::validator_for(&compact)?;
    for (document, valid) in [
        (json!({}), true),
        (json!({"kid": 1}), false),
        (json!({"kid": null}), false),
        (json!({"kid": "scalar"}), false),
        (json!({"kid": {}}), false),
        (json!({"kid": {"name": "present"}}), true),
    ] {
        sim_assert_eq!(have: flat_validator.is_valid(&document), want: valid);
        sim_assert_eq!(have: compact_validator.is_valid(&document), want: valid);
    }
    Ok(())
}

fn kid_object_carrier_schema(condition: &Value, definitions: &Value) -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": { "kid": {
            "type": "object",
            "additionalProperties": {},
            "allOf": [{ "if": condition, "then": { "required": ["name"] } }],
        } },
        "definitions": definitions,
    })
}

#[test]
fn a_large_disjunction_emits_one_common_prefix_and_distinct_residuals() -> eyre::Result<()> {
    large_shared_disjunction(false)
}

#[test]
fn a_large_disjunction_keeps_its_prefix_after_guards_are_appended() -> eyre::Result<()> {
    large_shared_disjunction(true)
}

fn large_shared_disjunction(appended: bool) -> eyre::Result<()> {
    let prefix_guards = (0..64)
        .map(|index| equals(&format!("flag{index:02}"), "on"))
        .collect::<Vec<_>>();
    let prefix = GuardPrefix::new(prefix_guards.clone());
    let extras = (0..100)
        .map(|index| equals("mode", &format!("case{index:03}")))
        .collect::<Vec<_>>();
    let values = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = test_absence(&values, &roots);
    let polarity = ConditionPolarity::Widen;
    let mut compact = ConditionFragmentCache::new();
    let tail = appended.then(|| equals("a-tail", "on"));
    let condition = EncodedConditionPlan::any_of(
        extras
            .iter()
            .cycle()
            .take(50_000)
            .map(|guard| {
                let mut row =
                    GuardConjunction::prefixed(vec![guard.clone()], prefix.clone(), Vec::new())
                        .canonicalized();
                if let Some(tail) = &tail {
                    row.push(tail.clone());
                }
                let encoded = compact.planned_conjunction(&row, &[], &values, absence, polarity);
                sim_assert_eq!(have:encoded.shared_lint_parts().is_some(),want:true);
                encoded
            })
            .collect(),
    );
    let actual = compact.materialize_condition(&condition).into_value();
    let mut flat = ConditionFragmentCache::new();
    let expected_prefix =
        SchemaNode::all_of(flat.clauses(&prefix_guards, &[], &values, absence, polarity))
            .into_value();
    let residuals = extras
        .iter()
        .map(|guard| {
            let residual = std::iter::once(guard.clone())
                .chain(tail.iter().cloned())
                .collect::<Vec<_>>();
            SchemaNode::all_of(flat.clauses(&residual, &[], &values, absence, polarity))
                .into_value()
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(have: &actual, want: &json!({"allOf": [
        {"$ref": "#/definitions/helm-condition-prefix-0"},
        {"anyOf": residuals},
    ]}));
    sim_assert_eq!(
        have: &compact.definitions,
        want: &BTreeMap::from([("helm-condition-prefix-0".to_owned(), expected_prefix)]),
    );
    let schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": compact.definitions,
        "allOf": [actual],
    });
    let validator = jsonschema::validator_for(&schema)?;
    let mut passing = (0..64)
        .map(|index| (format!("flag{index:02}"), json!("on")))
        .collect::<serde_json::Map<_, _>>();
    passing.insert("mode".to_owned(), json!("case099"));
    if appended {
        passing.insert("a-tail".to_owned(), json!("on"));
        let mut wrong_tail = passing.clone();
        wrong_tail.insert("a-tail".to_owned(), json!("off"));
        assert!(!validator.is_valid(&Value::Object(wrong_tail)));
    }
    assert!(validator.is_valid(&Value::Object(passing.clone())));
    let mut outside_residuals = passing.clone();
    outside_residuals.insert("mode".to_owned(), json!("case100"));
    assert!(!validator.is_valid(&Value::Object(outside_residuals)));
    passing.remove("flag37");
    assert!(!validator.is_valid(&Value::Object(passing)));
    Ok(())
}

#[test]
fn appended_guard_encoding_keeps_flat_order_names_and_filtered_positions() {
    let invalid = ConditionalGuard::MatchesPattern {
        path: ValuesPath::parse("bad"),
        pattern: "[".to_owned(),
    };
    let duplicate = not(equals("p", "off"));
    let prefix = GuardPrefix::new(vec![duplicate.clone(), invalid.clone(), equals("z", "on")]);
    let values = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = test_absence(&values, &roots);
    for polarity in [
        ConditionPolarity::Widen,
        ConditionPolarity::Narrow,
        ConditionPolarity::Exact,
    ] {
        let mut compact = ConditionFragmentCache::new();
        let mut flat = ConditionFragmentCache::new();
        for extra in ["zz", "zzz", "zz"] {
            let mut row = GuardConjunction::prefixed(
                vec![not(equals(extra, "off"))],
                prefix.clone(),
                Vec::new(),
            )
            .canonicalized();
            // These guards must remain after the sorted head, including the duplicate.
            for tail in [equals("a", "on"), duplicate.clone(), invalid.clone()] {
                row.push(tail);
            }
            let encoded = compact.planned_conjunction(&row, &[], &values, absence, polarity);
            let expected =
                SchemaNode::all_of(flat.clauses(&row.to_vec(), &[], &values, absence, polarity))
                    .into_value();
            sim_assert_eq!(have:encoded.flat_schema().into_value(),want:expected);
            sim_assert_eq!(have:&compact.definitions,want:&flat.definitions);
            sim_assert_eq!(have:encoded.shared_lint_parts().is_some(),want:true);
            sim_assert_eq!(have:row.canonical_parts().is_none(),want:true);
        }
    }
}
