//! Lint constraints judged part by part agree with the whole constraint
//! document.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::condition_encoding::{HELM_TRUTHY_DEFINITION_NAME, helm_truthy_definition_schema};
use crate::lint_validators::LintValidators;

/// The verdict of `if condition then then_value` compiled as one document
/// holding `definitions`, or `None` when it does not compile.
fn whole_document_verdicts(
    condition: &Value,
    then_value: &Value,
    definitions: &BTreeMap<String, Value>,
    instances: &[Value],
) -> Option<Vec<bool>> {
    let document = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$defs": definitions,
        "allOf": [{ "if": condition, "then": then_value }],
    });
    let validator = jsonschema::validator_for(&document).ok()?;
    Some(
        instances
            .iter()
            .map(|instance| validator.is_valid(instance))
            .collect(),
    )
}

fn part_verdicts(
    validators: &LintValidators<'_>,
    condition: &Value,
    then_value: &Value,
    conditions: &BTreeMap<String, Value>,
    instances: &[Value],
) -> Option<Vec<bool>> {
    let validator = validators.constraint_validator(condition, then_value, conditions)?;
    Some(
        instances
            .iter()
            .map(|instance| validator.is_valid(instance))
            .collect(),
    )
}

#[test]
fn parts_judge_as_the_whole_constraint_document() {
    let providers = BTreeMap::from([(
        "port".to_string(),
        json!({ "type": "integer", "minimum": 1 }),
    )]);
    let conditions = BTreeMap::from([
        (
            "helm-condition-0".to_string(),
            json!({ "properties": { "enabled": { "$ref": "#/$defs/t" } }, "required": ["enabled"] }),
        ),
        (
            "helm-condition-1".to_string(),
            json!({ "anyOf": [
                { "$ref": "#/$defs/helm-condition-0" },
                { "not": { "required": ["mode"] } },
            ] }),
        ),
        (
            "helm-condition-2".to_string(),
            json!({ "not": { "$ref": "#/$defs/helm-condition-1" } }),
        ),
    ]);
    let instances = [
        json!({}),
        json!({ "enabled": true }),
        json!({ "enabled": "" }),
        json!({ "enabled": 0, "mode": "a" }),
        json!({ "enabled": true, "port": 0 }),
        json!({ "enabled": true, "port": 8080 }),
        json!({ "mode": "a", "port": "x" }),
        json!(null),
    ];
    let constraints = [
        (
            json!({ "allOf": [
                { "$ref": "#/$defs/helm-condition-1" },
                { "properties": { "mode": { "const": "a" } } },
            ] }),
            json!({ "properties": { "port": { "$ref": "#/$defs/port" } }, "required": ["port"] }),
        ),
        (json!({ "$ref": "#/$defs/helm-condition-2" }), json!(false)),
        (
            json!({ "anyOf": [{ "$ref": "#/$defs/helm-condition-0" }, { "required": ["mode"] }] }),
            json!({ "properties": { "port": { "$ref": "#/$defs/port" } } }),
        ),
        (
            json!({ "properties": { "enabled": { "$ref": "#/$defs/t" } } }),
            json!({ "required": ["port"] }),
        ),
        // A dangling reference leaves the constraint uncompilable.
        (
            json!({ "$ref": "#/$defs/helm-condition-0" }),
            json!({ "properties": { "port": { "$ref": "#/$defs/missing" } } }),
        ),
        // A provider definition only a condition body references is not in
        // the constraint's document.
        (json!({ "$ref": "#/$defs/helm-condition-3" }), json!(true)),
    ];
    let mut conditions = conditions;
    conditions.insert(
        "helm-condition-3".to_string(),
        json!({ "properties": { "port": { "$ref": "#/$defs/port" } } }),
    );
    let validators = LintValidators::new(&providers);
    for (condition, then_value) in &constraints {
        let mut definitions = BTreeMap::new();
        let constraint = json!({ "if": condition, "then": then_value });
        let text = constraint.to_string();
        for (name, body) in &providers {
            if text.contains(&format!("#/$defs/{name}\"")) {
                definitions.insert(name.clone(), body.clone());
            }
        }
        // Every condition definition here reaches the others only through
        // the condition map, so the whole map stands in for its closure.
        if text.contains("helm-condition-") {
            definitions.extend(conditions.clone());
        }
        definitions.insert(
            HELM_TRUTHY_DEFINITION_NAME.to_string(),
            helm_truthy_definition_schema(),
        );
        sim_assert_eq!(
            have: part_verdicts(&validators, condition, then_value, &conditions, &instances),
            want: whole_document_verdicts(condition, then_value, &definitions, &instances)
        );
    }
}

/// Asserts that judging part by part agrees with the whole document
/// holding every provider definition the constraint names, every
/// condition definition and Helm truthiness.
fn assert_parts_judge_as_the_whole_document(
    providers: &BTreeMap<String, Value>,
    conditions: &BTreeMap<String, Value>,
    condition: &Value,
    then_value: &Value,
    instances: &[Value],
) {
    let text = json!({ "if": condition, "then": then_value }).to_string();
    let mut definitions = BTreeMap::new();
    for (name, body) in providers {
        if text.contains(&format!("#/$defs/{name}\"")) {
            definitions.insert(name.clone(), body.clone());
        }
    }
    definitions.extend(conditions.clone());
    definitions.insert(
        HELM_TRUTHY_DEFINITION_NAME.to_string(),
        helm_truthy_definition_schema(),
    );
    let validators = LintValidators::new(providers);
    sim_assert_eq!(
        have: part_verdicts(&validators, condition, then_value, conditions, instances),
        want: whole_document_verdicts(condition, then_value, &definitions, instances)
    );
}

/// The whole document never meta-validates its definitions, so an
/// expanded definition must not be checked either.
#[test]
fn a_definition_with_invalid_annotations_judges_as_in_the_whole_document() {
    let providers = BTreeMap::from([("x".to_string(), json!({ "type": "string", "title": 5 }))]);
    assert_parts_judge_as_the_whole_document(
        &providers,
        &BTreeMap::new(),
        &json!({ "required": ["a"] }),
        &json!({ "$ref": "#/$defs/x" }),
        &[json!({ "a": 1 }), json!("s"), json!({})],
    );
}

/// A reference to the document root resolves against the whole
/// constraint document.
#[test]
fn a_root_reference_judges_as_in_the_whole_document() {
    assert_parts_judge_as_the_whole_document(
        &BTreeMap::new(),
        &BTreeMap::new(),
        &json!({ "required": ["x"] }),
        &json!({ "properties": { "a": { "$ref": "#" } }, "required": ["a"] }),
        &[
            json!({ "x": 1, "a": {} }),
            json!({ "x": 1 }),
            json!({ "a": {} }),
        ],
    );
}
