//! JSON Schema output minimization.
//!
//! This crate is deliberately independent of Helm. It treats the input as a
//! JSON Schema document, finds repeated schema subtrees, and rewrites repeated
//! occurrences to internal `$defs` / `$ref` entries.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_json_schema_walk::{visit_subschemas, visit_subschemas_mut};
use serde_json::{Map, Value};

const DEFINITIONS_KEY: &str = "$defs";
const DEFINITION_NAME_PREFIX: &str = "schema";

/// Deduplicate repeated JSON Schema subtrees into root-level `$defs`.
///
/// Only schema-position objects are eligible. Data arrays and objects used as
/// keyword payloads, such as `required`, `enum`, `type`, or Kubernetes
/// extension metadata, are never replaced with `$ref`.
#[must_use]
#[tracing::instrument(skip_all)]
pub fn minimize_schema(schema: Value) -> Value {
    if !can_insert_generated_definitions(&schema) {
        return schema;
    }

    let mut candidates = BTreeMap::new();
    collect_candidates(&schema, true, &mut candidates);
    let planned = plan_definitions(&schema, candidates);
    if planned.is_empty() {
        return schema;
    }

    let mut schema = schema;
    let mut definitions = BTreeMap::new();
    rewrite_schema(&mut schema, true, &planned, &mut definitions);

    if !definitions.is_empty() {
        insert_definitions(&mut schema, definitions);
    }

    schema
}

fn can_insert_generated_definitions(schema: &Value) -> bool {
    match schema {
        Value::Object(object) => object
            .get(DEFINITIONS_KEY)
            .is_none_or(serde_json::Value::is_object),
        _ => false,
    }
}

fn collect_candidates(schema: &Value, is_root: bool, candidates: &mut BTreeMap<String, usize>) {
    if !is_root && let Some(canonical) = candidate_fingerprint(schema) {
        *candidates.entry(canonical).or_insert(0) += 1;
    }

    visit_subschemas(schema, &mut |subschema| {
        collect_candidates(subschema, false, candidates);
    });
}

fn plan_definitions(
    schema: &Value,
    candidates: BTreeMap<String, usize>,
) -> BTreeMap<String, String> {
    let mut existing_names = existing_definition_names(schema);
    let mut repeated: Vec<(String, usize)> = candidates
        .into_iter()
        .filter(|(_, occurrences)| *occurrences > 1)
        .collect();
    // Largest subtree first (the canonical string is the subtree, so its
    // length is the subtree's byte size), then most occurrences.
    repeated.sort_by(|(left_canonical, left), (right_canonical, right)| {
        right_canonical
            .len()
            .cmp(&left_canonical.len())
            .then_with(|| right.cmp(left))
            .then_with(|| left_canonical.cmp(right_canonical))
    });

    let mut planned = BTreeMap::new();
    let mut next_id = 1usize;
    for (canonical, occurrences) in repeated {
        let (name, following_id) = next_definition_name(&existing_names, next_id);
        if estimated_savings(canonical.len(), occurrences, &name) <= 0 {
            continue;
        }
        existing_names.insert(name.clone());
        next_id = following_id;
        planned.insert(canonical, name);
    }
    planned
}

fn next_definition_name(existing_names: &BTreeSet<String>, next_id: usize) -> (String, usize) {
    let mut id = next_id;
    loop {
        let candidate = format!("{DEFINITION_NAME_PREFIX}{id}");
        id += 1;
        if !existing_names.contains(&candidate) {
            return (candidate, id);
        }
    }
}

fn estimated_savings(schema_bytes: usize, occurrences: usize, name: &str) -> i128 {
    let ref_bytes =
        helm_schema_json_schema_walk::canonical_json_string(&reference_schema(name)).len();
    let original = schema_bytes.saturating_mul(occurrences);
    let rewritten = schema_bytes
        .saturating_add(ref_bytes.saturating_mul(occurrences))
        .saturating_add(name.len())
        .saturating_add(DEFINITIONS_KEY.len())
        .saturating_add(16);
    original as i128 - rewritten as i128
}

fn rewrite_schema(
    schema: &mut Value,
    is_root: bool,
    planned: &BTreeMap<String, String>,
    definitions: &mut BTreeMap<String, Value>,
) {
    if !is_root
        && let Some(canonical) = candidate_fingerprint(schema)
        && let Some(definition_name) = planned.get(&canonical)
    {
        definitions
            .entry(definition_name.clone())
            .or_insert_with(|| schema.clone());
        *schema = reference_schema(definition_name);
        return;
    }

    visit_subschemas_mut(schema, &mut |subschema| {
        rewrite_schema(subschema, false, planned, definitions);
    });
}

fn insert_definitions(schema: &mut Value, definitions: BTreeMap<String, Value>) {
    let Value::Object(root) = schema else {
        return;
    };
    let entry = root
        .entry(DEFINITIONS_KEY.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(existing) = entry else {
        return;
    };
    for (name, value) in definitions {
        existing.insert(name, value);
    }
}

fn candidate_fingerprint(schema: &Value) -> Option<String> {
    if !matches!(schema, Value::Object(_)) || contains_reference_scope_keyword(schema) {
        return None;
    }
    Some(helm_schema_json_schema_walk::canonical_json_string(schema))
}

fn contains_reference_scope_keyword(value: &Value) -> bool {
    let Value::Object(object) = value else {
        return false;
    };
    if object.keys().any(|key| {
        matches!(
            key.as_str(),
            "$ref"
                | "$id"
                | "id"
                | "$anchor"
                | "$dynamicRef"
                | "$dynamicAnchor"
                | "$recursiveRef"
                | "$recursiveAnchor"
                | "$defs"
                | "definitions"
        )
    }) {
        return true;
    }

    let mut contains_scope = false;
    visit_subschemas(value, &mut |subschema| {
        contains_scope |= contains_reference_scope_keyword(subschema);
    });
    contains_scope
}

fn existing_definition_names(schema: &Value) -> BTreeSet<String> {
    schema
        .get(DEFINITIONS_KEY)
        .and_then(Value::as_object)
        .map(|definitions| definitions.keys().cloned().collect())
        .unwrap_or_default()
}

fn reference_schema(name: &str) -> Value {
    Value::Object(Map::from_iter([(
        "$ref".to_string(),
        Value::String(format!("#/{DEFINITIONS_KEY}/{name}")),
    )]))
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
