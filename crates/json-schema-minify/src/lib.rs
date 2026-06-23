//! JSON Schema output minimization.
//!
//! This crate is deliberately independent of Helm. It treats the input as a
//! JSON Schema document, finds repeated schema subtrees, and rewrites repeated
//! occurrences to internal `$defs` / `$ref` entries.

use std::collections::{BTreeMap, BTreeSet};

use json_schema_walk::{visit_subschemas, visit_subschemas_mut};
use serde_json::{Map, Value};

const DEFAULT_MIN_SUBTREE_BYTES: usize = 1;
const DEFINITIONS_KEY: &str = "$defs";
const DEFINITION_NAME_PREFIX: &str = "schema";

/// Options for [`minimize_schema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimizeOptions {
    /// Smallest compact serialized subschema size considered for extraction.
    ///
    /// The minimizer still requires an estimated output-size win before adding
    /// a definition, so the default considers every object subschema.
    pub min_subtree_bytes: usize,
}

impl Default for MinimizeOptions {
    fn default() -> Self {
        Self {
            min_subtree_bytes: DEFAULT_MIN_SUBTREE_BYTES,
        }
    }
}

/// Summary of a minimization run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MinimizeStats {
    /// Number of generated `$defs` entries inserted into the root schema.
    pub definitions_added: usize,
    /// Number of subschema occurrences replaced by a `$ref`.
    pub replacements: usize,
    /// Compact JSON byte size before minimization.
    pub bytes_before: usize,
    /// Compact JSON byte size after minimization.
    pub bytes_after: usize,
}

/// Result of [`minimize_schema`].
#[derive(Debug, Clone, PartialEq)]
pub struct MinimizeResult {
    /// The minimized schema document.
    pub schema: Value,
    /// Statistics describing what changed.
    pub stats: MinimizeStats,
}

#[derive(Debug, Clone)]
struct Candidate {
    occurrences: usize,
    bytes: usize,
}

#[derive(Debug)]
struct CandidateFingerprint {
    canonical: String,
    bytes: usize,
}

#[derive(Debug, Clone)]
struct PlannedDefinition {
    name: String,
}

/// Deduplicate repeated JSON Schema subtrees into root-level `$defs`.
///
/// Only schema-position objects are eligible. Data arrays and objects used as
/// keyword payloads, such as `required`, `enum`, `type`, or Kubernetes
/// extension metadata, are never replaced with `$ref`.
#[must_use]
#[tracing::instrument(skip_all, fields(min_subtree_bytes = options.min_subtree_bytes))]
pub fn minimize_schema(schema: Value, options: &MinimizeOptions) -> MinimizeResult {
    let bytes_before = compact_json_len(&schema);
    if !can_insert_generated_definitions(&schema) {
        return MinimizeResult {
            schema,
            stats: MinimizeStats {
                bytes_before,
                bytes_after: bytes_before,
                ..MinimizeStats::default()
            },
        };
    }

    let mut candidates = BTreeMap::new();
    collect_candidates(&schema, true, options.min_subtree_bytes, &mut candidates);
    let planned = plan_definitions(&schema, candidates);
    if planned.is_empty() {
        return MinimizeResult {
            schema,
            stats: MinimizeStats {
                bytes_before,
                bytes_after: bytes_before,
                ..MinimizeStats::default()
            },
        };
    }

    let mut schema = schema;
    let mut definitions = BTreeMap::new();
    let mut replacements = 0;
    rewrite_schema(
        &mut schema,
        true,
        &planned,
        options.min_subtree_bytes,
        &mut definitions,
        &mut replacements,
    );

    let definitions_added = definitions.len();
    if definitions_added > 0 {
        insert_definitions(&mut schema, definitions);
    }

    let bytes_after = compact_json_len(&schema);
    MinimizeResult {
        schema,
        stats: MinimizeStats {
            definitions_added,
            replacements,
            bytes_before,
            bytes_after,
        },
    }
}

fn can_insert_generated_definitions(schema: &Value) -> bool {
    match schema {
        Value::Object(object) => object
            .get(DEFINITIONS_KEY)
            .is_none_or(serde_json::Value::is_object),
        _ => false,
    }
}

fn collect_candidates(
    schema: &Value,
    is_root: bool,
    min_subtree_bytes: usize,
    candidates: &mut BTreeMap<String, Candidate>,
) {
    if !is_root && let Some(fingerprint) = candidate_fingerprint(schema, min_subtree_bytes) {
        candidates
            .entry(fingerprint.canonical)
            .and_modify(|candidate| candidate.occurrences += 1)
            .or_insert_with(|| Candidate {
                occurrences: 1,
                bytes: fingerprint.bytes,
            });
    }

    visit_subschemas(schema, &mut |subschema| {
        collect_candidates(subschema, false, min_subtree_bytes, candidates);
    });
}

fn plan_definitions(
    schema: &Value,
    candidates: BTreeMap<String, Candidate>,
) -> BTreeMap<String, PlannedDefinition> {
    let mut existing_names = existing_definition_names(schema);
    let mut repeated: Vec<(String, Candidate)> = candidates
        .into_iter()
        .filter(|(_, candidate)| candidate.occurrences > 1)
        .collect();
    repeated.sort_by(|(left_key, left), (right_key, right)| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| right.occurrences.cmp(&left.occurrences))
            .then_with(|| left_key.cmp(right_key))
    });

    let mut planned = BTreeMap::new();
    let mut next_id = 1usize;
    for (canonical, candidate) in repeated {
        let (name, following_id) = next_definition_name(&existing_names, next_id);
        if estimated_savings(candidate.bytes, candidate.occurrences, &name) <= 0 {
            continue;
        }
        existing_names.insert(name.clone());
        next_id = following_id;
        planned.insert(canonical, PlannedDefinition { name });
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

fn estimated_savings(schema_bytes: usize, occurrences: usize, name: &str) -> isize {
    let ref_bytes = compact_json_len(&reference_schema(name));
    let original = schema_bytes.saturating_mul(occurrences);
    let rewritten = schema_bytes
        .saturating_add(ref_bytes.saturating_mul(occurrences))
        .saturating_add(name.len())
        .saturating_add(DEFINITIONS_KEY.len())
        .saturating_add(16);
    original as isize - rewritten as isize
}

fn rewrite_schema(
    schema: &mut Value,
    is_root: bool,
    planned: &BTreeMap<String, PlannedDefinition>,
    min_subtree_bytes: usize,
    definitions: &mut BTreeMap<String, Value>,
    replacements: &mut usize,
) {
    if !is_root
        && let Some(fingerprint) = candidate_fingerprint(schema, min_subtree_bytes)
        && let Some(definition) = planned.get(&fingerprint.canonical)
    {
        definitions
            .entry(definition.name.clone())
            .or_insert_with(|| schema.clone());
        *schema = reference_schema(&definition.name);
        *replacements += 1;
        return;
    }

    visit_subschemas_mut(schema, &mut |subschema| {
        rewrite_schema(
            subschema,
            false,
            planned,
            min_subtree_bytes,
            definitions,
            replacements,
        );
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

fn candidate_fingerprint(schema: &Value, min_subtree_bytes: usize) -> Option<CandidateFingerprint> {
    if !matches!(schema, Value::Object(_)) || contains_reference_scope_keyword(schema) {
        return None;
    }
    let canonical = canonical_compact_json_string(schema);
    let bytes = canonical.len();
    (bytes >= min_subtree_bytes).then_some(CandidateFingerprint { canonical, bytes })
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

fn compact_json_len(value: &Value) -> usize {
    canonical_compact_json_string(value).len()
}

fn canonical_compact_json_string(value: &Value) -> String {
    serde_json::to_string(&canonicalize_json_value(value))
        .expect("serialize canonical serde_json value")
}

fn canonicalize_json_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut out = Map::new();
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            for key in keys {
                if let Some(value) = object.get(key) {
                    out.insert(key.clone(), canonicalize_json_value(value));
                }
            }
            Value::Object(out)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json_value).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
