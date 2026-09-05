//! JSON Schema output minimization.
//!
//! This crate is deliberately independent of Helm. It treats the input as a
//! JSON Schema document, finds repeated schema subtrees, and rewrites repeated
//! occurrences to internal `$defs` / `$ref` entries.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_json_schema_walk::{
    ReferenceSiblings, SchemaMetadataIndex, SchemaTraversalContext, try_map_schema_context,
    visit_subschemas, visit_subschemas_mut,
};
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
pub fn minimize_schema(mut schema: Value) -> Value {
    if !can_insert_generated_definitions(&schema) {
        return schema;
    }

    normalize_logical_schema(&mut schema);
    let Some(protected) = protected_reference_targets(&schema) else {
        return schema;
    };
    let metadata = SchemaMetadataIndex::new(&schema);
    let mut fingerprint_counts = HashMap::new();
    collect_candidate_fingerprints(
        &schema,
        &metadata,
        &protected,
        true,
        &mut fingerprint_counts,
    );
    fingerprint_counts.retain(|_, occurrences| *occurrences > 1);
    let mut candidates = HashMap::new();
    collect_exact_candidates(
        &schema,
        &metadata,
        &protected,
        true,
        &fingerprint_counts,
        &mut candidates,
    );
    let existing_names = existing_definition_names(&schema);
    let planned = plan_definitions(existing_names, candidates);
    if planned.is_empty() {
        return schema;
    }

    let mut definitions = BTreeMap::new();
    let mut minimized = rewrite_schema(&schema, &metadata, &protected, &planned, &mut definitions);
    inline_unprofitable_definitions(&mut minimized, &mut definitions, &planned);

    if !definitions.is_empty() {
        definitions = compact_definition_names(&mut minimized, definitions);
        insert_definitions(&mut minimized, definitions);
    }

    minimized
}

fn can_insert_generated_definitions(schema: &Value) -> bool {
    match schema {
        Value::Object(object) => object
            .get(DEFINITIONS_KEY)
            .is_none_or(serde_json::Value::is_object),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct CandidateFingerprint {
    digest: u128,
    byte_len: usize,
}

#[derive(Debug)]
struct ExactCandidate {
    schema: Value,
    canonical: String,
    occurrences: usize,
}

#[derive(Debug)]
struct PlannedDefinition {
    schema: Value,
    name: String,
}

#[derive(Debug, Default)]
struct PlannedDefinitions {
    by_fingerprint: HashMap<CandidateFingerprint, Vec<PlannedDefinition>>,
}

impl PlannedDefinitions {
    fn is_empty(&self) -> bool {
        self.by_fingerprint.is_empty()
    }

    fn definition_name(
        &self,
        fingerprint: CandidateFingerprint,
        schema: &Value,
    ) -> Option<&String> {
        self.by_fingerprint
            .get(&fingerprint)?
            .iter()
            .find(|definition| definition.schema == *schema)
            .map(|definition| &definition.name)
    }
}

fn collect_candidate_fingerprints(
    schema: &Value,
    metadata: &SchemaMetadataIndex,
    protected: &BTreeSet<usize>,
    is_root: bool,
    candidates: &mut HashMap<CandidateFingerprint, usize>,
) {
    if !is_root && has_reference_scope(schema) {
        return;
    }
    if !is_root
        && !protected.contains(&value_address(schema))
        && let Some(fingerprint) = candidate_fingerprint(schema, metadata)
    {
        *candidates.entry(fingerprint).or_insert(0) += 1;
    }

    visit_subschemas(schema, ReferenceSiblings::Visit, &mut |subschema| {
        collect_candidate_fingerprints(subschema, metadata, protected, false, candidates);
    });
}

fn collect_exact_candidates(
    schema: &Value,
    metadata: &SchemaMetadataIndex,
    protected: &BTreeSet<usize>,
    is_root: bool,
    selected: &HashMap<CandidateFingerprint, usize>,
    candidates: &mut HashMap<CandidateFingerprint, Vec<ExactCandidate>>,
) {
    if !is_root && has_reference_scope(schema) {
        return;
    }
    if !is_root
        && !protected.contains(&value_address(schema))
        && let Some(fingerprint) = candidate_fingerprint(schema, metadata)
        && selected.contains_key(&fingerprint)
    {
        let bucket = candidates.entry(fingerprint).or_default();
        if let Some(candidate) = bucket
            .iter_mut()
            .find(|candidate| candidate.schema == *schema)
        {
            candidate.occurrences += 1;
        } else {
            bucket.push(ExactCandidate {
                schema: schema.clone(),
                canonical: helm_schema_json_schema_walk::canonical_json_string(schema),
                occurrences: 1,
            });
        }
    }

    visit_subschemas(schema, ReferenceSiblings::Visit, &mut |subschema| {
        collect_exact_candidates(subschema, metadata, protected, false, selected, candidates);
    });
}

fn plan_definitions(
    mut existing_names: BTreeSet<String>,
    candidates: HashMap<CandidateFingerprint, Vec<ExactCandidate>>,
) -> PlannedDefinitions {
    let mut repeated: Vec<(CandidateFingerprint, ExactCandidate)> = candidates
        .into_iter()
        .flat_map(|(fingerprint, candidates)| {
            candidates
                .into_iter()
                .filter(|candidate| candidate.occurrences > 1)
                .map(move |candidate| (fingerprint, candidate))
        })
        .collect();
    // Largest subtree first (the canonical string is the subtree, so its
    // length is the subtree's byte size), then most occurrences.
    repeated.sort_by(|(_, left), (_, right)| {
        right
            .canonical
            .len()
            .cmp(&left.canonical.len())
            .then_with(|| right.occurrences.cmp(&left.occurrences))
            .then_with(|| left.canonical.cmp(&right.canonical))
    });

    let mut planned = PlannedDefinitions::default();
    let mut next_id = 1usize;
    for (fingerprint, candidate) in repeated {
        let (name, following_id) = next_definition_name(&existing_names, next_id);
        if estimated_savings(candidate.canonical.len(), candidate.occurrences, &name) <= 0 {
            continue;
        }
        existing_names.insert(name.clone());
        next_id = following_id;
        planned
            .by_fingerprint
            .entry(fingerprint)
            .or_default()
            .push(PlannedDefinition {
                schema: candidate.schema,
                name,
            });
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
    schema: &Value,
    metadata: &SchemaMetadataIndex,
    protected: &BTreeSet<usize>,
    planned: &PlannedDefinitions,
    definitions: &mut BTreeMap<String, Value>,
) -> Value {
    let rewritten = try_map_schema_context(
        schema,
        SchemaTraversalContext::Schema,
        |node, context, depth| {
            if context != SchemaTraversalContext::Schema {
                return Ok::<_, std::convert::Infallible>(None);
            }
            if depth > 0 && has_reference_scope(node) {
                return Ok(Some(node.clone()));
            }
            if depth > 0
                && !protected.contains(&value_address(node))
                && let Some(fingerprint) = candidate_fingerprint(node, metadata)
                && let Some(name) = planned.definition_name(fingerprint, node)
            {
                if !definitions.contains_key(name) {
                    // Traverse the immutable representative so nested bodies share
                    // the same metadata and cannot select their own root again.
                    let body = rewrite_schema(node, metadata, protected, planned, definitions);
                    definitions.insert(name.clone(), body);
                }
                return Ok(Some(reference_schema(name)));
            }
            Ok(None)
        },
    );
    match rewritten {
        Ok(schema) => schema,
        Err(error) => match error {},
    }
}

fn compact_definition_names(
    schema: &mut Value,
    definitions: BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let generated_names = definitions.keys().cloned().collect::<BTreeSet<_>>();
    let mut reference_counts = BTreeMap::new();
    count_generated_references(schema, &generated_names, &mut reference_counts);
    for definition in definitions.values() {
        count_generated_references(definition, &generated_names, &mut reference_counts);
    }

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

fn inline_unprofitable_definitions(
    schema: &mut Value,
    definitions: &mut BTreeMap<String, Value>,
    planned: &PlannedDefinitions,
) {
    let names = definitions.keys().cloned().collect::<BTreeSet<_>>();
    let mut counts = BTreeMap::new();
    count_generated_references(schema, &names, &mut counts);
    for body in definitions.values() {
        count_generated_references(body, &names, &mut counts);
    }
    // A generated body references only strictly smaller extracted subtrees.
    // Resolve its children before measuring the body's actual emitted size.
    let mut order = planned
        .by_fingerprint
        .iter()
        .flat_map(|(fingerprint, candidates)| {
            candidates
                .iter()
                .map(|candidate| (fingerprint.byte_len, candidate.name.clone()))
        })
        .collect::<Vec<_>>();
    order.sort();
    let mut removed = BTreeMap::new();
    for (_, name) in order {
        let Some(mut body) = definitions.remove(&name) else {
            continue;
        };
        inline_generated_references(&mut body, &removed);
        let uses = counts.get(&name).copied().unwrap_or_default();
        if estimated_savings(body.to_string().len(), uses, &name) > 0 {
            definitions.insert(name, body);
            continue;
        }
        let mut children = BTreeMap::new();
        count_generated_references(&body, &names, &mut children);
        for (child, references) in children {
            let count = counts.entry(child).or_default();
            *count = count
                .saturating_sub(references)
                .saturating_add(references.saturating_mul(uses));
        }
        removed.insert(format!("{DEFINITION_REF_PREFIX}{name}"), body);
    }
    inline_generated_references(schema, &removed);
}

fn inline_generated_references(schema: &mut Value, replacements: &BTreeMap<String, Value>) {
    if replacements.is_empty() {
        return;
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        && let Some(replacement) = replacements.get(reference)
    {
        // Replacement bodies already contain their resolved smaller subtrees.
        // Substitution is one step, so inserted bodies are not visited again.
        schema.clone_from(replacement);
        return;
    }
    visit_subschemas_mut(schema, ReferenceSiblings::Visit, &mut |subschema| {
        if !has_reference_scope(subschema) {
            inline_generated_references(subschema, replacements);
        }
    });
}

fn count_generated_references(
    schema: &Value,
    generated_names: &BTreeSet<String>,
    counts: &mut BTreeMap<String, usize>,
) {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        && let Some(name) = reference.strip_prefix(DEFINITION_REF_PREFIX)
        && generated_names.contains(name)
    {
        *counts.entry(name.to_string()).or_default() += 1;
    }
    visit_subschemas(schema, ReferenceSiblings::Visit, &mut |subschema| {
        if !has_reference_scope(subschema) {
            count_generated_references(subschema, generated_names, counts);
        }
    });
}

fn rewrite_generated_references(schema: &mut Value, references: &BTreeMap<String, String>) {
    if let Some(Value::String(reference)) = schema.get_mut("$ref")
        && let Some(replacement) = references.get(reference)
    {
        reference.clone_from(replacement);
    }
    visit_subschemas_mut(schema, ReferenceSiblings::Visit, &mut |subschema| {
        if !has_reference_scope(subschema) {
            rewrite_generated_references(subschema, references);
        }
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

fn candidate_fingerprint(
    schema: &Value,
    metadata: &SchemaMetadataIndex,
) -> Option<CandidateFingerprint> {
    if !matches!(schema, Value::Object(_)) {
        return None;
    }
    let metadata = metadata.get(schema)?;
    if metadata.contains_unsafe_reference_scope_keyword() {
        return None;
    }
    let canonical = metadata.canonical();
    Some(CandidateFingerprint {
        digest: canonical.digest(),
        byte_len: canonical.byte_len(),
    })
}

fn normalize_logical_schema(schema: &mut Value) {
    let Some(protected) = protected_reference_targets(schema) else {
        return;
    };
    normalize_logical_schema_at(schema, &protected, true);
}

fn normalize_logical_schema_at(schema: &mut Value, protected: &BTreeSet<usize>, is_root: bool) {
    if !is_root && has_reference_scope(schema) {
        return;
    }
    visit_subschemas_mut(schema, ReferenceSiblings::Visit, &mut |child| {
        normalize_logical_schema_at(child, protected, false);
    });
    let Value::Object(object) = schema else {
        return;
    };
    for keyword in ["allOf", "anyOf"] {
        let Some(array) = object.get_mut(keyword) else {
            continue;
        };
        if protected.contains(&value_address(array)) {
            continue;
        }
        let Value::Array(items) = array else {
            continue;
        };
        let mut flattened = Vec::new();
        for mut item in std::mem::take(items) {
            let nested = match &mut item {
                Value::Object(child) if child.len() == 1 => {
                    child.get_mut(keyword).and_then(Value::as_array_mut)
                }
                _ => None,
            };
            if let Some(nested) = nested {
                flattened.append(nested);
            } else {
                flattened.push(item);
            }
        }
        let mut buckets = BTreeMap::<u128, Vec<Value>>::new();
        for item in flattened {
            let bucket = buckets.entry(logical_sort_digest(&item)).or_default();
            if !bucket.contains(&item) {
                bucket.push(item);
            }
        }
        *items = buckets
            .into_values()
            .flat_map(|mut bucket| {
                if bucket.len() > 1 {
                    bucket.sort_by_cached_key(helm_schema_json_schema_walk::canonical_json_string);
                }
                bucket
            })
            .collect();
    }
}

fn value_address(value: &Value) -> usize {
    std::ptr::from_ref(value).addr()
}

fn has_reference_scope(schema: &Value) -> bool {
    schema.as_object().is_some_and(|object| {
        object.keys().any(|key| {
            matches!(
                key.as_str(),
                "$id"
                    | "id"
                    | "$anchor"
                    | "$dynamicAnchor"
                    | "$recursiveAnchor"
                    | "$dynamicRef"
                    | "$recursiveRef"
                    | "$schema"
            )
        })
    })
}

/// Protect the address-bearing ancestors of reference targets from relocation.
///
/// References outside the document require a resource resolver, so this local
/// transformation leaves those documents unchanged.
/// Targets outside lexical schema positions can introduce unindexed references,
/// so documents using those targets are also left unchanged.
fn protected_reference_targets(schema: &Value) -> Option<BTreeSet<usize>> {
    fn collect(
        node: &Value,
        nested_resource: bool,
        references: &mut Vec<String>,
        schema_positions: &mut HashSet<usize>,
    ) {
        schema_positions.insert(value_address(node));
        if let Some(reference) = node.get("$ref").and_then(Value::as_str)
            && (!nested_resource || !reference.starts_with('#'))
        {
            references.push(reference.to_string());
        }
        visit_subschemas(node, ReferenceSiblings::Visit, &mut |child| {
            let nested_resource =
                nested_resource || child.get("$id").is_some() || child.get("id").is_some();
            collect(child, nested_resource, references, schema_positions);
        });
    }

    let mut references = Vec::new();
    let mut schema_positions = HashSet::new();
    collect(schema, false, &mut references, &mut schema_positions);
    let mut protected = BTreeSet::new();
    for reference in references {
        let fragment = reference.strip_prefix('#')?;
        let pointer = percent_encoding::percent_decode_str(fragment)
            .decode_utf8()
            .ok()?;
        if !pointer.is_empty() && !pointer.starts_with('/') {
            continue;
        }
        let target = schema.pointer(&pointer)?;
        if !schema_positions.contains(&value_address(target)) {
            return None;
        }
        protected.insert(value_address(schema));
        for (index, _) in pointer.match_indices('/').skip(1) {
            protected.insert(value_address(schema.pointer(&pointer[..index])?));
        }
    }
    Some(protected)
}

fn logical_sort_digest(value: &Value) -> u128 {
    fn update(hash: &mut u128, value: &Value) {
        match value {
            Value::Null => update_bytes(hash, &[0]),
            Value::Bool(value) => update_bytes(hash, &[1, u8::from(*value)]),
            Value::Number(value) => {
                update_bytes(hash, &[2]);
                update_sized_bytes(hash, value.to_string().as_bytes());
            }
            Value::String(value) => {
                update_bytes(hash, &[3]);
                update_sized_bytes(hash, value.as_bytes());
            }
            Value::Array(values) => {
                update_bytes(hash, &[4]);
                update_len(hash, values.len());
                for value in values {
                    update(hash, value);
                }
            }
            Value::Object(object) => {
                update_bytes(hash, &[5]);
                update_len(hash, object.len());
                let mut keys = object.keys().collect::<Vec<_>>();
                keys.sort();
                for key in keys {
                    update_sized_bytes(hash, key.as_bytes());
                    if let Some(value) = object.get(key) {
                        update(hash, value);
                    }
                }
            }
        }
    }

    fn update_len(hash: &mut u128, len: usize) {
        update_bytes(hash, &u64::try_from(len).unwrap_or(u64::MAX).to_be_bytes());
    }

    fn update_sized_bytes(hash: &mut u128, bytes: &[u8]) {
        update_len(hash, bytes.len());
        update_bytes(hash, bytes);
    }

    fn update_bytes(hash: &mut u128, bytes: &[u8]) {
        const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
        for byte in bytes {
            *hash ^= u128::from(*byte);
            *hash = hash.wrapping_mul(FNV_PRIME);
        }
    }

    let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    update(&mut hash, value);
    hash
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
