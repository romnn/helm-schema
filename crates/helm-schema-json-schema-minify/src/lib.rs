//! JSON Schema output minimization.
//!
//! This crate is deliberately independent of Helm. It treats the input as a
//! JSON Schema document, finds repeated schema subtrees, and rewrites repeated
//! occurrences to internal `$defs` / `$ref` entries.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_json_schema_walk::{visit_subschemas, visit_subschemas_mut};
use serde_json::{Map, Value};

const DEFINITIONS_KEY: &str = "$defs";
const DEFINITION_REF_PREFIX: &str = "#/$defs/";

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

    let mut candidate_schema = schema.clone();
    remove_definitions(&mut candidate_schema);
    let mut candidates = BTreeMap::new();
    collect_candidates(&candidate_schema, true, &mut candidates);
    let planned = plan_definitions(&schema, candidates);
    if planned.is_empty() {
        return schema;
    }

    let mut schema = schema;
    let existing_definitions = remove_definitions(&mut schema);
    let mut definitions = BTreeMap::new();
    rewrite_schema(&mut schema, true, &planned, &mut definitions);
    insert_definitions(&mut schema, existing_definitions);

    if !definitions.is_empty() {
        definitions = compact_definition_names(&mut schema, definitions);
        insert_definitions(&mut schema, definitions);
    }

    schema
}

fn remove_definitions(schema: &mut Value) -> BTreeMap<String, Value> {
    let Some(definitions) = schema
        .as_object_mut()
        .and_then(|root| root.remove(DEFINITIONS_KEY))
        .and_then(|definitions| definitions.as_object().cloned())
    else {
        return BTreeMap::new();
    };
    definitions.into_iter().collect()
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
        let candidate = base62(id);
        id += 1;
        if !existing_names.contains(&candidate) {
            return (candidate, id);
        }
    }
}

fn base62(mut value: usize) -> String {
    const DIGITS: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

    let mut reversed = Vec::new();
    loop {
        let Some(digit) = DIGITS.get(value % DIGITS.len()) else {
            return String::new();
        };
        reversed.push(char::from(*digit));
        value /= DIGITS.len();
        if value == 0 {
            break;
        }
    }
    reversed.into_iter().rev().collect()
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

fn compact_definition_names(
    schema: &mut Value,
    definitions: BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let generated_names = definitions.keys().cloned().collect::<BTreeSet<_>>();
    let mut reference_counts = BTreeMap::new();
    count_generated_references(schema, &generated_names, &mut reference_counts);

    let mut ranked = definitions.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|(left_name, _), (right_name, _)| {
        reference_counts
            .get(right_name)
            .copied()
            .unwrap_or_default()
            .cmp(&reference_counts.get(left_name).copied().unwrap_or_default())
            .then_with(|| left_name.cmp(right_name))
    });

    let mut existing_names = existing_definition_names(schema);
    let mut next_id = 1;
    let mut renamed = Vec::with_capacity(ranked.len());
    let mut references = BTreeMap::new();
    for (old_name, definition) in ranked {
        let (new_name, following_id) = next_definition_name(&existing_names, next_id);
        existing_names.insert(new_name.clone());
        next_id = following_id;
        references.insert(
            format!("{DEFINITION_REF_PREFIX}{old_name}"),
            format!("{DEFINITION_REF_PREFIX}{new_name}"),
        );
        renamed.push((new_name, definition));
    }

    rewrite_generated_references(schema, &references);
    renamed
        .into_iter()
        .map(|(name, mut definition)| {
            rewrite_generated_references(&mut definition, &references);
            (name, definition)
        })
        .collect()
}

fn count_generated_references(
    schema: &Value,
    generated_names: &BTreeSet<String>,
    counts: &mut BTreeMap<String, usize>,
) {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if let Some(name) = reference.strip_prefix(DEFINITION_REF_PREFIX)
            && generated_names.contains(name)
        {
            *counts.entry(name.to_string()).or_default() += 1;
        }
        return;
    }
    visit_subschemas(schema, &mut |subschema| {
        count_generated_references(subschema, generated_names, counts);
    });
}

fn rewrite_generated_references(schema: &mut Value, references: &BTreeMap<String, String>) {
    if let Some(Value::String(reference)) = schema.get_mut("$ref") {
        if let Some(replacement) = references.get(reference) {
            reference.clone_from(replacement);
        }
        return;
    }
    visit_subschemas_mut(schema, &mut |subschema| {
        rewrite_generated_references(subschema, references);
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
    if !matches!(schema, Value::Object(_)) || contains_unsafe_reference_scope_keyword(schema) {
        return None;
    }
    Some(helm_schema_json_schema_walk::canonical_json_string(schema))
}

fn contains_unsafe_reference_scope_keyword(value: &Value) -> bool {
    let Value::Object(object) = value else {
        return false;
    };
    if let Some(reference) = object.get("$ref")
        && !reference
            .as_str()
            .is_some_and(|reference| reference.starts_with(DEFINITION_REF_PREFIX))
    {
        return true;
    }
    if object.keys().any(|key| {
        matches!(
            key.as_str(),
            "$id"
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
        contains_scope |= contains_unsafe_reference_scope_keyword(subschema);
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
