use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{ConditionalGuard, GuardConjunction, GuardPrefix, GuardValue, ValuesPath};
use serde_json::{Value, json};
use serde_yaml::Value as YamlValue;
use test_util::prelude::sim_assert_eq;

use crate::condition_encoding::{
    AbsenceDefaults, ConditionFragmentCache, ConditionPolarity, EncodedConjunction,
};

use super::LintValidators;

fn equals(path: &str, text: &str) -> ConditionalGuard {
    ConditionalGuard::Eq {
        path: ValuesPath::parse(path),
        value: GuardValue::String(text.to_owned()),
    }
}

fn encode(
    cache: &mut ConditionFragmentCache,
    prefix: &GuardPrefix,
    extras: Vec<ConditionalGuard>,
) -> EncodedConjunction {
    static EMPTY: YamlValue = YamlValue::Null;
    static ROOTS: BTreeSet<Vec<String>> = BTreeSet::new();

    let row = GuardConjunction::prefixed(extras, prefix.clone(), Vec::new()).canonicalized();
    let absence = AbsenceDefaults {
        runtime_defaults: &EMPTY,
        dependency_refill: &EMPTY,
        dependency_roots: &ROOTS,
    };
    cache.planned_conjunction(&row, &[], &EMPTY, absence, ConditionPolarity::Exact)
}

fn check_verdicts(
    validators: &LintValidators<'_>,
    plan: &EncodedConjunction,
    then_value: &Value,
    conditions: &BTreeMap<String, Value>,
    instances: &[Value],
) -> Option<Vec<bool>> {
    let actual = validators
        .planned_constraint_validator(plan, then_value, conditions)
        .map(|validator| {
            instances
                .iter()
                .map(|value| validator.is_valid(value))
                .collect::<Vec<_>>()
        });
    let flat = plan.flat_schema().into_value();
    let expected = validators
        .constraint_validator(&flat, then_value, conditions)
        .map(|validator| {
            instances
                .iter()
                .map(|value| validator.is_valid(value))
                .collect::<Vec<_>>()
        });
    sim_assert_eq!(have: &actual, want: &expected);
    actual
}

#[test]
fn closed_prefix_is_reused_and_keeps_its_definitions_visible_to_then() -> eyre::Result<()> {
    let prefix = GuardPrefix::new(vec![
        ConditionalGuard::Truthy {
            path: ValuesPath::parse("flag"),
        },
        ConditionalGuard::Not(Arc::new(equals("mode", "off"))),
    ]);
    let mut cache = ConditionFragmentCache::new();
    let red = encode(&mut cache, &prefix, vec![equals("a", "red")]);
    let blue = encode(&mut cache, &prefix, vec![equals("z", "blue")]);
    let providers = BTreeMap::from([(
        "target".to_owned(),
        json!({"$ref": "#/definitions/helm-condition-0/not"}),
    )]);
    let mut validators = LintValidators::new(&providers);
    let then_value = json!({"$ref": "#/definitions/target"});
    let instances = [
        json!({"flag": true, "mode": "on", "a": "red"}),
        json!({"flag": true, "mode": "on", "z": "blue"}),
        json!({"flag": true, "mode": "off", "a": "red", "z": "blue"}),
        json!({"flag": false, "mode": "on", "a": "red", "z": "blue"}),
        json!(null),
    ];
    for (plan, expected) in [
        (&red, vec![false, true, true, true, true]),
        (&blue, vec![true, false, true, true, true]),
    ] {
        let actual = check_verdicts(
            &validators,
            plan,
            &then_value,
            &cache.definitions,
            &instances,
        );
        sim_assert_eq!(have: actual, want: Some(expected));
    }

    sim_assert_eq!(have: validators.prefixes.borrow().len(), want: 1);
    let cached_judge = validators
        .prefixes
        .borrow()
        .values()
        .next()
        .and_then(|entry| entry.closed.as_ref()?.judge.as_ref())
        .map(Rc::downgrade)
        .ok_or_eyre("retain the compiled judge for the shared closed prefix")?;
    validators.release_compiled_parts();
    assert!(cached_judge.upgrade().is_none());
    let rebuilt = check_verdicts(
        &validators,
        &red,
        &then_value,
        &cache.definitions,
        &instances,
    );
    sim_assert_eq!(have: rebuilt, want: Some(vec![false, true, true, true, true]));
    Ok(())
}

#[test]
fn truthiness_aliases_keep_the_whole_constraints_reference_winners() {
    let mut cache = ConditionFragmentCache::new();
    let prefix = GuardPrefix::new(vec![ConditionalGuard::Truthy {
        path: ValuesPath::parse("flag"),
    }]);
    let plan = encode(&mut cache, &prefix, Vec::new());
    let providers = BTreeMap::from([("t".to_owned(), json!({"anyOf": [true]}))]);
    let validators = LintValidators::new(&providers);
    let then_value = json!({"$ref": "#/definitions/t/anyOf/0"});
    let instances = [json!({"flag": true}), json!({"flag": false})];
    let actual = check_verdicts(
        &validators,
        &plan,
        &then_value,
        &cache.definitions,
        &instances,
    );
    sim_assert_eq!(have: actual, want: Some(vec![false, true]));
    assert!(
        validators
            .prefixes
            .borrow()
            .values()
            .all(|entry| entry.closed.is_none())
    );

    let mut cache = ConditionFragmentCache::new();
    let prefix = GuardPrefix::new(vec![ConditionalGuard::Not(Arc::new(equals("mode", "off")))]);
    let plan = encode(&mut cache, &prefix, Vec::new());
    cache
        .definitions
        .insert("helm-condition-0".to_owned(), then_value);
    cache
        .definitions
        .insert("t".to_owned(), json!({"anyOf": [true]}));
    let providers = BTreeMap::new();
    let validators = LintValidators::new(&providers);
    let then_value = json!({"not": {"$ref": "#/definitions/t"}});
    let actual = check_verdicts(
        &validators,
        &plan,
        &then_value,
        &cache.definitions,
        &instances,
    );
    sim_assert_eq!(have: actual, want: Some(vec![true, true]));
    assert!(
        validators
            .prefixes
            .borrow()
            .values()
            .all(|entry| entry.closed.is_none())
    );
}

#[test]
fn missing_and_document_relative_references_use_the_legacy_compilation() {
    let mut cache = ConditionFragmentCache::new();
    let prefix = GuardPrefix::new(vec![ConditionalGuard::Not(Arc::new(equals("mode", "off")))]);
    let plan = encode(&mut cache, &prefix, vec![equals("extra", "yes")]);
    let providers = BTreeMap::new();
    let instances = [
        json!({}),
        json!({"mode": "on", "extra": "yes"}),
        json!({"mode": "off"}),
    ];
    let validators = LintValidators::new(&providers);
    let actual = check_verdicts(
        &validators,
        &plan,
        &json!({"$ref": "#/allOf/0/if"}),
        &cache.definitions,
        &instances,
    );
    sim_assert_eq!(have: actual, want: Some(vec![true, true, true]));

    for (body, then_value, expected) in [
        (json!({"$ref": "#/definitions/missing"}), json!(false), None),
        (
            json!({"$ref": "#/allOf/0/then"}),
            json!(false),
            Some(vec![true, true, true]),
        ),
    ] {
        let mut conditions = cache.definitions.clone();
        conditions.insert("helm-condition-0".to_owned(), body);
        let validators = LintValidators::new(&providers);
        let actual = check_verdicts(&validators, &plan, &then_value, &conditions, &instances);
        sim_assert_eq!(have: actual, want: expected);
        assert!(
            validators
                .prefixes
                .borrow()
                .values()
                .all(|entry| entry.closed.is_none())
        );
    }

    let mut conditions = cache.definitions;
    conditions.insert(
        "helm-condition-0".to_owned(),
        json!({"$id": "urn:prefix-condition", "required": ["flag"]}),
    );
    let validators = LintValidators::new(&providers);
    let _ = check_verdicts(
        &validators,
        &plan,
        &json!({"$ref": "urn:prefix-condition"}),
        &conditions,
        &instances,
    );
    assert!(
        validators
            .prefixes
            .borrow()
            .values()
            .all(|entry| entry.closed.is_none())
    );
}
