use std::collections::{BTreeMap, BTreeSet};

use helm_schema_json_schema_walk::{
    GENERATED_DEFINITIONS, ReferenceSiblings, visit_subschemas_mut,
};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

/// Canonicalizes associativity and the union of terminal rejection conditions.
///
/// All atomic schemas and nonterminal claims remain part of the full comparison.
pub(super) fn canonical(schema: &Value) -> Value {
    let mut schema = schema.clone();
    let mut conditions = BTreeMap::new();
    if let Some(definitions) = schema
        .get(GENERATED_DEFINITIONS.as_str())
        .and_then(Value::as_object)
    {
        for (name, body) in definitions {
            if name.starts_with("helm-condition-") {
                conditions.insert(name.clone(), body.clone());
            }
        }
    }
    normalize(&mut schema, &conditions, &mut BTreeSet::new(), true);
    if let Some(definitions) = schema
        .get_mut(GENERATED_DEFINITIONS.as_str())
        .and_then(Value::as_object_mut)
    {
        definitions.retain(|name, _| !conditions.contains_key(name));
        if definitions.is_empty() {
            schema
                .as_object_mut()
                .expect("schema object")
                .remove(GENERATED_DEFINITIONS.as_str());
        }
    }
    schema
}

fn normalize(
    schema: &mut Value,
    conditions: &BTreeMap<String, Value>,
    active: &mut BTreeSet<String>,
    root: bool,
) {
    if let Some(object) = schema.as_object() {
        assert!(
            !object.contains_key("id")
                && !object.contains_key("$id")
                && !object.contains_key("$dynamicRef")
                && !object.contains_key("$recursiveRef"),
            "rejection comparison requires one schema resource"
        );
        if let Some(dialect) = object.get("$schema") {
            assert!(root, "nested dialect changes are outside this comparison");
            sim_assert_eq!(have: dialect, want: "http://json-schema.org/draft-07/schema#");
        }
    }
    if let Some(object) = schema.as_object()
        && let Some(reference) = object.get("$ref").and_then(Value::as_str)
    {
        let reference = helm_schema_json_schema_walk::parse_definition_reference(reference)
            .expect("rejection comparison requires local definition references");
        assert!(
            reference.suffix.is_empty(),
            "definition suffix is not expandable"
        );
        if reference.keyword == GENERATED_DEFINITIONS
            && let Some(body) = conditions.get(&reference.name)
        {
            sim_assert_eq!(have: object.len(), want: 1);
            assert!(
                active.insert(reference.name.clone()),
                "recursive private condition"
            );
            *schema = body.clone();
            normalize(schema, conditions, active, false);
            active.remove(&reference.name);
            return;
        }
    }
    visit_subschemas_mut(schema, ReferenceSiblings::Visit, &mut |child| {
        normalize(child, conditions, active, false);
    });
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    for operator in ["allOf", "anyOf"] {
        if let Some(terms) = object.get_mut(operator).and_then(Value::as_array_mut) {
            *terms = flatten(operator, std::mem::take(terms));
        }
    }
    if let Some(terms) = object.get_mut("allOf").and_then(Value::as_array_mut) {
        let mut rejections = Vec::new();
        terms.retain(|term| {
            if let Some(clause) = term.as_object()
                && clause.len() == 2
                && clause.get("then") == Some(&Value::Bool(false))
                && let Some(condition) = clause.get("if")
            {
                rejections.push(condition.clone());
                false
            } else {
                true
            }
        });
        if !rejections.is_empty() {
            let mut union = json!({"anyOf": flatten("anyOf", rejections)});
            collapse(&mut union);
            terms.push(json!({"if": union, "then": false}));
        }
        terms.sort_unstable_by_key(Value::to_string);
        terms.dedup();
    }
    collapse(schema);
}

fn flatten(operator: &str, terms: Vec<Value>) -> Vec<Value> {
    assert!(!terms.is_empty(), "Draft 7 logical arrays must be nonempty");
    let mut flattened = Vec::new();
    for mut term in terms {
        if let Some(object) = term.as_object_mut()
            && object.len() == 1
            && let Some(nested) = object.remove(operator)
        {
            flattened.extend(nested.as_array().expect("logical schema array").clone());
        } else {
            flattened.push(term);
        }
    }
    flattened.sort_unstable_by_key(Value::to_string);
    flattened.dedup();
    flattened
}

fn collapse(schema: &mut Value) {
    if let Some(object) = schema.as_object()
        && object.len() == 1
    {
        for operator in ["allOf", "anyOf"] {
            if let Some(terms) = object.get(operator).and_then(Value::as_array)
                && let [term] = terms.as_slice()
            {
                *schema = term.clone();
                return;
            }
        }
    }
}

#[test]
fn terminal_normalization_preserves_every_atomic_constraint() {
    let first = json!({"type": "string"});
    let second = json!({"type": "number"});
    let separate = json!({"allOf": [
        {"if": first, "then": false},
        {"if": second, "then": false},
    ]});
    let shared = json!({"allOf": [{"if": {"anyOf": [
        {"anyOf": [second]}, first,
    ]}, "then": false}]});
    sim_assert_eq!(have: canonical(&shared), want: canonical(&separate));
    let changed = json!({"if": {"type": "boolean"}, "then": false});
    assert_ne!(canonical(&shared), canonical(&changed));
}

#[test]
fn terminal_normalization_refuses_references_with_observable_layout() {
    for schema in [
        json!({"definitions": {"helm-condition-0": {"properties": {"p": false}}},
            "allOf": [{"$ref": "#/definitions/helm-condition-0/properties/p"}]}),
        json!({"definitions": {"helm-condition-0": {"type": "string"}},
            "$ref": "#/definitions/helm-condition-0", "title": "sibling"}),
        json!({"allOf": [true, false], "properties": {"x": {"$ref": "#/allOf/0"}}}),
        json!({"definitions": {"helm-condition-0": {"$ref": "#/definitions/helm-condition-0"}}}),
        json!({"properties": {"x": {"$id": "other", "type": "string"}}}),
        json!({"$dynamicRef": "#node"}),
        json!({"allOf": [{"allOf": []}, {"type": "string"}]}),
    ] {
        assert!(std::panic::catch_unwind(|| canonical(&schema)).is_err());
    }
}
