use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use helm_schema_core::{ProviderOrigin, ProviderSchemaSource};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::overlay_lowering::ConditionalResolvedSchema;
use crate::path_resolver::ResolvedPathSchema;
use crate::provider_schema::{ProviderSchemaCandidate, rewrite_internal_refs_for_root_definition};

const DEFINITIONS_KEY: &str = "$defs";
const PROVIDER_DEFINITION_PREFIX: &str = "providerSchema";
const PROVIDER_SOURCE_DEFINITION_PREFIX: &str = "providerSource";
const PROVIDER_SHARED_DEFINITION_PREFIX: &str = "providerShared";
const MIN_SHARED_PROVIDER_PAYLOAD_BYTES: usize = 16 * 1024;

/// Extract repeated provider-owned schema leaves into root `$defs`
/// definitions, rewriting each extracted `resolved_path.schema` to an
/// internal `$ref`. Returns the definitions keyed by definition name.
#[tracing::instrument(skip_all)]
#[tracing::instrument(skip_all)]
pub(crate) fn extract_provider_definitions(
    resolved_paths: &mut [ResolvedPathSchema],
    conditional_schemas: &mut [ConditionalResolvedSchema],
    values_descriptions: &BTreeMap<String, String>,
) -> BTreeMap<String, Value> {
    let description_paths = DescriptionPathIndex::new(values_descriptions);
    let entries = ProviderSchemaDefinitionEntries::from_resolved_paths_and_conditionals(
        resolved_paths,
        conditional_schemas,
        &description_paths,
    );
    let mut ref_names_by_key = BTreeMap::new();
    let mut definitions_by_name = BTreeMap::new();
    let mut used_definition_names = BTreeSet::new();
    let mut next_id = 1;

    for (key, entry) in entries.into_repeated_entries() {
        let name = next_definition_name(&entry, &mut used_definition_names, &mut next_id);
        ref_names_by_key.insert(key, name.clone());
        let definition_schema = entry.into_definition_schema(&name);
        definitions_by_name.insert(name, definition_schema);
    }

    // A `$ref` is only a faithful substitute while the site still carries the
    // candidate payload verbatim. Resolve policy may have processed the site
    // schema (default-acceptance unions, falsy off-states, values merges);
    // those sites keep their inline schema, and the whole-document
    // repeated-payload pass still shares any large payload embedded inside.
    for resolved_path in resolved_paths {
        let Some(provider_schema_candidate) = resolved_path.provider_schema_candidate.as_ref()
        else {
            continue;
        };
        if description_paths.has_description_at_or_below(&resolved_path.path_segments) {
            continue;
        }
        if resolved_path.schema != *provider_schema_candidate.schema() {
            continue;
        }
        let Some(name) = ref_names_by_key.get(provider_schema_candidate.key()) else {
            continue;
        };
        resolved_path.schema = reference_schema(name);
    }
    for conditional in conditional_schemas {
        let Some(provider_schema_candidate) = conditional.provider_schema_candidate.as_ref() else {
            continue;
        };
        let target_segments = crate::split_value_path(&conditional.target_value_path);
        if description_paths.has_description_at_or_below(&target_segments) {
            continue;
        }
        if conditional.target_schema != *provider_schema_candidate.schema() {
            continue;
        }
        let Some(name) = ref_names_by_key.get(provider_schema_candidate.key()) else {
            continue;
        };
        conditional.target_schema = reference_schema(name);
    }

    definitions_by_name
}

pub(crate) fn insert_definitions_into_root(
    schema: &mut Value,
    definitions_by_name: BTreeMap<String, Value>,
) {
    if definitions_by_name.is_empty() {
        return;
    }

    let Value::Object(root) = schema else {
        return;
    };
    let definitions = root
        .entry(DEFINITIONS_KEY.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(definitions) = definitions else {
        return;
    };

    for (name, definition) in definitions_by_name {
        definitions.insert(name, definition);
    }
}

#[derive(Debug)]
struct RepeatedPayload {
    schema: Value,
    uses: usize,
}

pub(crate) fn extract_repeated_provider_payloads(schema: &mut Value) -> BTreeMap<String, Value> {
    // Serializing every subtree's canonical form is O(size x depth); count
    // candidates with a bottom-up structural hash plus an exact canonical
    // byte length instead, and materialize the true canonical string only
    // for the handful of repeated large cores (naming stays sorted by that
    // string, so the emitted definitions are unchanged).
    let mut counts = std::collections::HashMap::<u64, usize>::new();
    visit_repeated_core_hashes(schema, &mut |core_hash, core_len| {
        if core_len >= MIN_SHARED_PROVIDER_PAYLOAD_BYTES {
            *counts.entry(core_hash).or_insert(0) += 1;
        }
    });
    counts.retain(|_, uses| *uses > 1);

    let mut payloads = BTreeMap::<String, RepeatedPayload>::new();
    collect_selected_schema_cores(schema, &counts, &mut payloads);

    let selected = payloads
        .into_iter()
        .filter(|(_, payload)| payload.uses > 1)
        .enumerate()
        .map(|(index, (key, payload))| {
            (
                key,
                (
                    format!("{PROVIDER_SHARED_DEFINITION_PREFIX}{}", index + 1),
                    payload.schema,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut used = BTreeSet::new();
    replace_repeated_schema_cores(schema, &counts, &selected, &mut used);

    selected
        .into_values()
        .filter(|(name, _)| used.contains(name))
        .collect()
}

/// Bottom-up structural hash and exact canonical-serialization byte length.
///
/// Equal canonical strings imply equal hashes and lengths; only leaves are
/// serialized, so the whole document costs one linear pass.
fn canonical_hash_len(value: &Value) -> (u64, usize) {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let len = match value {
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            let mut len = 2 + keys.len().saturating_sub(1);
            0u8.hash(&mut hasher);
            for key in keys {
                let Some(child) = object.get(key) else {
                    continue;
                };
                let (child_hash, child_len) = canonical_hash_len(child);
                key.hash(&mut hasher);
                child_hash.hash(&mut hasher);
                len += json_string_len(key) + 1 + child_len;
            }
            len
        }
        Value::Array(items) => {
            let mut len = 2 + items.len().saturating_sub(1);
            1u8.hash(&mut hasher);
            for item in items {
                let (child_hash, child_len) = canonical_hash_len(item);
                child_hash.hash(&mut hasher);
                len += child_len;
            }
            len
        }
        scalar => {
            let text = serde_json::to_string(scalar).unwrap_or_default();
            2u8.hash(&mut hasher);
            text.hash(&mut hasher);
            text.len()
        }
    };
    (hasher.finish(), len)
}

/// Hash and length of one object's CORE (its non-decoration entries), reusing
/// the full-value hashes of the retained children.
fn core_hash_len(object: &Map<String, Value>) -> (u64, usize) {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    0u8.hash(&mut hasher);
    let mut keys: Vec<_> = object
        .keys()
        .filter(|key| !is_schema_decoration(key))
        .collect();
    keys.sort();
    let mut len = 2 + keys.len().saturating_sub(1);
    for key in keys {
        let Some(child) = object.get(key) else {
            continue;
        };
        let (child_hash, child_len) = canonical_hash_len(child);
        key.hash(&mut hasher);
        child_hash.hash(&mut hasher);
        len += json_string_len(key) + 1 + child_len;
    }
    (hasher.finish(), len)
}

/// Exact `serde_json` string-encoding length (quotes plus escapes).
fn json_string_len(text: &str) -> usize {
    let mut len = 2;
    for byte in text.bytes() {
        len += match byte {
            b'"' | b'\\' | 0x08 | 0x09 | 0x0a | 0x0c | 0x0d => 2,
            0x00..=0x1f => 6,
            _ => 1,
        };
    }
    len
}

fn visit_repeated_core_hashes(schema: &Value, record: &mut impl FnMut(u64, usize)) {
    let Value::Object(object) = schema else {
        return;
    };
    let (core_hash, core_len) = core_hash_len(object);
    record(core_hash, core_len);
    visit_schema_children(object, |child| {
        visit_repeated_core_hashes(child, record);
    });
}

/// Materialize cores and their canonical strings only for hash-selected
/// candidates; the canonical-string map keeps the original naming order.
fn collect_selected_schema_cores(
    schema: &Value,
    counts: &std::collections::HashMap<u64, usize>,
    payloads: &mut BTreeMap<String, RepeatedPayload>,
) {
    let Value::Object(object) = schema else {
        return;
    };
    let (core_hash, core_len) = core_hash_len(object);
    if core_len >= MIN_SHARED_PROVIDER_PAYLOAD_BYTES && counts.contains_key(&core_hash) {
        let core = schema_core(object);
        let key = helm_schema_json_schema_walk::canonical_json_string(&core);
        let payload = payloads.entry(key).or_insert_with(|| RepeatedPayload {
            schema: core,
            uses: 0,
        });
        payload.uses += 1;
    }
    visit_schema_children(object, |child| {
        collect_selected_schema_cores(child, counts, payloads);
    });
}

fn replace_repeated_schema_cores(
    schema: &mut Value,
    counts: &std::collections::HashMap<u64, usize>,
    selected: &BTreeMap<String, (String, Value)>,
    used: &mut BTreeSet<String>,
) {
    let Value::Object(object) = schema else {
        return;
    };
    let (core_hash, core_len) = core_hash_len(object);
    if core_len >= MIN_SHARED_PROVIDER_PAYLOAD_BYTES
        && counts.contains_key(&core_hash)
        && let core = schema_core(object)
        && let key = helm_schema_json_schema_walk::canonical_json_string(&core)
        && let Some((name, _)) = selected.get(&key)
    {
        let mut replacement = schema_decorations(object);
        replacement.insert(
            "allOf".to_string(),
            Value::Array(vec![reference_schema(name)]),
        );
        *object = replacement;
        used.insert(name.clone());
        return;
    }
    visit_schema_children_mut(object, |child| {
        replace_repeated_schema_cores(child, counts, selected, used);
    });
}

fn schema_core(object: &Map<String, Value>) -> Value {
    Value::Object(
        object
            .iter()
            .filter(|(key, _)| !is_schema_decoration(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

fn schema_decorations(object: &Map<String, Value>) -> Map<String, Value> {
    object
        .iter()
        .filter(|(key, _)| is_schema_decoration(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn is_schema_decoration(key: &str) -> bool {
    crate::schema_model::is_annotation_keyword(key) || key == "$comment" || key.starts_with("x-")
}

fn visit_schema_children(object: &Map<String, Value>, mut visit: impl FnMut(&Value)) {
    for key in ["properties", "patternProperties", "definitions", "$defs"] {
        if let Some(children) = object.get(key).and_then(Value::as_object) {
            for child in children.values() {
                visit(child);
            }
        }
    }
    for key in [
        "additionalProperties",
        "additionalItems",
        "contains",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
    ] {
        if let Some(child) = object.get(key) {
            visit(child);
        }
    }
    if let Some(items) = object.get("items") {
        if let Some(items) = items.as_array() {
            for item in items {
                visit(item);
            }
        } else {
            visit(items);
        }
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(children) = object.get(key).and_then(Value::as_array) {
            for child in children {
                visit(child);
            }
        }
    }
}

fn visit_schema_children_mut(object: &mut Map<String, Value>, mut visit: impl FnMut(&mut Value)) {
    for key in ["properties", "patternProperties", "definitions", "$defs"] {
        if let Some(children) = object.get_mut(key).and_then(Value::as_object_mut) {
            for child in children.values_mut() {
                visit(child);
            }
        }
    }
    for key in [
        "additionalProperties",
        "additionalItems",
        "contains",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
    ] {
        if let Some(child) = object.get_mut(key) {
            visit(child);
        }
    }
    if let Some(items) = object.get_mut("items") {
        if let Some(items) = items.as_array_mut() {
            for item in items {
                visit(item);
            }
        } else {
            visit(items);
        }
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(children) = object.get_mut(key).and_then(Value::as_array_mut) {
            for child in children {
                visit(child);
            }
        }
    }
}

#[derive(Debug, Default)]
struct ProviderSchemaDefinitionEntries {
    by_key: BTreeMap<String, ProviderSchemaDefinitionEntry>,
}

impl ProviderSchemaDefinitionEntries {
    fn from_resolved_paths_and_conditionals(
        resolved_paths: &[ResolvedPathSchema],
        conditional_schemas: &[ConditionalResolvedSchema],
        description_paths: &DescriptionPathIndex,
    ) -> Self {
        let mut entries = Self::default();
        for resolved_path in resolved_paths {
            let Some(provider_schema_candidate) = resolved_path.provider_schema_candidate.as_ref()
            else {
                continue;
            };
            if description_paths.has_description_at_or_below(&resolved_path.path_segments) {
                continue;
            }
            if !provider_schema_candidate.is_definition_candidate() {
                continue;
            }
            entries.insert(provider_schema_candidate);
        }
        for conditional in conditional_schemas {
            let Some(provider_schema_candidate) = conditional.provider_schema_candidate.as_ref()
            else {
                continue;
            };
            let target_segments = crate::split_value_path(&conditional.target_value_path);
            if description_paths.has_description_at_or_below(&target_segments) {
                continue;
            }
            if !provider_schema_candidate.is_definition_candidate() {
                continue;
            }
            entries.insert(provider_schema_candidate);
        }
        entries
    }

    fn insert(&mut self, provider_schema_candidate: &ProviderSchemaCandidate) {
        debug_assert!(
            provider_schema_candidate
                .source()
                .is_none_or(|source| !source.filename().is_empty()),
            "provider source metadata must name the source document"
        );
        let entry = self
            .by_key
            .entry(provider_schema_candidate.key().to_string())
            .or_insert_with(|| ProviderSchemaDefinitionEntry {
                schema: provider_schema_candidate.schema().clone(),
                definition_schemas_by_key: BTreeMap::new(),
                definition_schema_uses: 0,
                source_definition_names_by_identity: BTreeMap::new(),
                uses: 0,
            });
        if let Some(source_schema) = provider_schema_candidate.source_definition_schema() {
            entry.definition_schemas_by_key.insert(
                helm_schema_json_schema_walk::canonical_json_string(source_schema),
                source_schema.clone(),
            );
            entry.definition_schema_uses += 1;
        }
        if let Some(source) = provider_schema_candidate.source() {
            entry.source_definition_names_by_identity.insert(
                ProviderSourceIdentity::from(source),
                source_definition_name(source),
            );
        }
        entry.uses += 1;
    }

    fn into_repeated_entries(
        self,
    ) -> impl Iterator<Item = (String, ProviderSchemaDefinitionEntry)> {
        self.by_key.into_iter().filter(|(_, entry)| entry.uses > 1)
    }
}

#[derive(Debug)]
struct ProviderSchemaDefinitionEntry {
    schema: Value,
    definition_schemas_by_key: BTreeMap<String, Value>,
    definition_schema_uses: usize,
    source_definition_names_by_identity: BTreeMap<ProviderSourceIdentity, String>,
    uses: usize,
}

impl ProviderSchemaDefinitionEntry {
    fn into_definition_schema(self, definition_name: &str) -> Value {
        if self.definition_schemas_by_key.len() == 1
            && self.definition_schema_uses == self.uses
            && let Some((_, schema)) = self.definition_schemas_by_key.into_iter().next()
            && let Some(schema) =
                rewrite_internal_refs_for_root_definition(&schema, definition_name)
        {
            return schema;
        }
        self.schema
    }

    fn preferred_source_definition_name(&self) -> Option<&str> {
        if self.source_definition_names_by_identity.len() == 1 {
            self.source_definition_names_by_identity
                .values()
                .next()
                .map(String::as_str)
        } else {
            None
        }
    }
}

#[derive(Debug, Default)]
struct DescriptionPathIndex {
    paths: Vec<Vec<String>>,
}

impl DescriptionPathIndex {
    fn new(descriptions: &BTreeMap<String, String>) -> Self {
        let paths = descriptions
            .iter()
            .filter(|(_, description)| !description.trim().is_empty())
            .map(|(path, _)| crate::split_value_path(path))
            .collect();
        Self { paths }
    }

    fn has_description_at_or_below(&self, path_segments: &[String]) -> bool {
        self.paths
            .iter()
            .any(|description_path| description_path.starts_with(path_segments))
    }
}

fn reference_schema(name: &str) -> Value {
    Value::Object(
        [(
            "$ref".to_string(),
            Value::String(format!("#/{DEFINITIONS_KEY}/{name}")),
        )]
        .into_iter()
        .collect(),
    )
}

fn next_definition_name(
    entry: &ProviderSchemaDefinitionEntry,
    used_names: &mut BTreeSet<String>,
    next_id: &mut usize,
) -> String {
    if let Some(source_name) = entry.preferred_source_definition_name() {
        return unique_definition_name(source_name, used_names);
    }

    loop {
        let name = format!("{PROVIDER_DEFINITION_PREFIX}{next_id}");
        *next_id += 1;
        if used_names.insert(name.clone()) {
            return name;
        }
    }
}

fn unique_definition_name(base_name: &str, used_names: &mut BTreeSet<String>) -> String {
    if used_names.insert(base_name.to_string()) {
        return base_name.to_string();
    }

    let mut suffix = 2;
    loop {
        let name = format!("{base_name}_{suffix}");
        suffix += 1;
        if used_names.insert(name.clone()) {
            return name;
        }
    }
}

fn source_definition_name(source: &ProviderSchemaSource) -> String {
    format!(
        "{PROVIDER_SOURCE_DEFINITION_PREFIX}_{}_{}",
        source_origin_label(source.origin()),
        source_fingerprint(source),
    )
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct ProviderSourceIdentity {
    origin: ProviderOrigin,
    source_id: String,
    version: Option<String>,
    filename: String,
    pointer: String,
}

impl From<&ProviderSchemaSource> for ProviderSourceIdentity {
    fn from(source: &ProviderSchemaSource) -> Self {
        Self {
            origin: source.origin(),
            source_id: source.source_id().to_string(),
            version: source.version().map(str::to_string),
            filename: source.filename().to_string(),
            pointer: source.pointer().to_string(),
        }
    }
}

fn source_origin_label(origin: ProviderOrigin) -> &'static str {
    match origin {
        ProviderOrigin::KubernetesOpenApi => "k8s",
        ProviderOrigin::DefaultCatalog => "crd_catalog",
        ProviderOrigin::ChartLocalCrd => "chart_crd",
        ProviderOrigin::LocalOverride => "override",
    }
}

fn source_fingerprint(source: &ProviderSchemaSource) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_origin_label(source.origin()).as_bytes());
    hasher.update([0]);
    hasher.update(source.source_id().as_bytes());
    hasher.update([0]);
    if let Some(version) = source.version() {
        hasher.update(version.as_bytes());
    }
    hasher.update([0]);
    hasher.update(source.filename().as_bytes());
    hasher.update([0]);
    hasher.update(source.pointer().as_bytes());

    // 12 hex chars = the digest's first 6 bytes; sha2 0.11's output array no longer implements
    // `LowerHex`, so format the bytes directly.
    let mut digest = String::with_capacity(12);
    for byte in hasher.finalize().iter().take(6) {
        let _ = write!(digest, "{byte:02x}");
    }
    digest
}

#[cfg(test)]
#[path = "tests/provider_definitions.rs"]
mod tests;
