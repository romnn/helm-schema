use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use helm_schema_core::{ProviderOrigin, ProviderSchemaSource};
use helm_schema_json_schema_walk::SchemaMetadataIndex;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::overlay_lowering::LoweredConjunct;
use crate::path_resolver::ResolvedPathSchema;
use crate::provider_schema::{ProviderSchemaCandidate, rewrite_internal_refs_for_root_definition};
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;

const DEFINITIONS_KEY: &str = "$defs";
const PROVIDER_DEFINITION_PREFIX: &str = "providerSchema";
const PROVIDER_SOURCE_DEFINITION_PREFIX: &str = "providerSource";
const PROVIDER_SHARED_DEFINITION_PREFIX: &str = "providerShared";
const MIN_SHARED_PROVIDER_PAYLOAD_BYTES: usize = 16 * 1024;

/// Extract repeated provider-owned schema leaves into root `$defs`
/// definitions, rewriting each extracted `resolved_path.schema` to an
/// internal `$ref`. Returns the definitions keyed by definition name.
#[tracing::instrument(skip_all)]
pub(crate) fn extract_provider_definitions(
    resolved_paths: &mut [ResolvedPathSchema],
    conditional_schemas: &mut [LoweredConjunct],
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
        let Some(provider_schema_candidate) = conditional.provider_candidate.as_ref() else {
            continue;
        };
        let target_segments = conditional
            .carrier
            .target_value_path
            .segments()
            .map(helm_schema_core::Segment::encode_component)
            .collect::<Vec<_>>();
        if description_paths.has_description_at_or_below(&target_segments) {
            continue;
        }
        if conditional.schema.clone().into_value() != *provider_schema_candidate.schema() {
            continue;
        }
        let Some(name) = ref_names_by_key.get(provider_schema_candidate.key()) else {
            continue;
        };
        conditional.schema = SchemaNode::reference(format!("#/{DEFINITIONS_KEY}/{name}"));
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

pub(crate) fn prune_unreachable_provider_definitions(
    document: &SchemaDocument,
    definitions_by_name: &mut BTreeMap<String, Value>,
) -> usize {
    let mut reachable = BTreeSet::new();
    document.visit_embedded_values(&mut |value| {
        collect_referenced_definitions(value, definitions_by_name, &mut reachable);
    });

    let mut pending = reachable.iter().cloned().collect::<Vec<_>>();
    while let Some(name) = pending.pop() {
        let Some(definition) = definitions_by_name.get(&name) else {
            continue;
        };
        let mut referenced = BTreeSet::new();
        collect_referenced_definitions(definition, definitions_by_name, &mut referenced);
        for referenced_name in referenced {
            if reachable.insert(referenced_name.clone()) {
                pending.push(referenced_name);
            }
        }
    }

    let before = definitions_by_name.len();
    definitions_by_name.retain(|name, _| reachable.contains(name));
    before - definitions_by_name.len()
}

fn collect_referenced_definitions(
    value: &Value,
    definitions_by_name: &BTreeMap<String, Value>,
    referenced: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str)
                && let Some(name) = root_definition_name(reference)
                && definitions_by_name.contains_key(name)
            {
                referenced.insert(name.to_string());
            }
            for child in object.values() {
                collect_referenced_definitions(child, definitions_by_name, referenced);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_referenced_definitions(item, definitions_by_name, referenced);
            }
        }
        _ => {}
    }
}

fn root_definition_name(reference: &str) -> Option<&str> {
    reference.strip_prefix("#/$defs/")?.split('/').next()
}

#[derive(Debug)]
struct RepeatedPayload {
    schema: Value,
    uses: usize,
}

pub(crate) fn extract_repeated_provider_payloads(schema: &mut Value) -> BTreeMap<String, Value> {
    let metadata = SchemaMetadataIndex::new(schema);
    let mut counts = std::collections::HashMap::<(u128, usize), usize>::new();
    visit_repeated_core_hashes(schema, &metadata, &mut |fingerprint| {
        if fingerprint.1 >= MIN_SHARED_PROVIDER_PAYLOAD_BYTES {
            *counts.entry(fingerprint).or_insert(0) += 1;
        }
    });
    counts.retain(|_, uses| *uses > 1);

    let mut payloads = BTreeMap::<String, RepeatedPayload>::new();
    collect_selected_schema_cores(schema, &metadata, &counts, &mut payloads);

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
    replace_repeated_schema_cores(schema, &metadata, &counts, &selected, &mut used);

    selected
        .into_values()
        .filter(|(name, _)| used.contains(name))
        .collect()
}

fn core_fingerprint(
    object: &Map<String, Value>,
    metadata: &SchemaMetadataIndex,
) -> Option<(u128, usize)> {
    metadata
        .object_excluding(object, is_schema_decoration)
        .map(|metadata| (metadata.digest(), metadata.byte_len()))
}

fn visit_repeated_core_hashes(
    schema: &Value,
    metadata: &SchemaMetadataIndex,
    record: &mut impl FnMut((u128, usize)),
) {
    let Value::Object(object) = schema else {
        return;
    };
    if let Some(fingerprint) = core_fingerprint(object, metadata) {
        record(fingerprint);
    }
    visit_schema_children(object, |child| {
        visit_repeated_core_hashes(child, metadata, record);
    });
}

/// Materialize cores and their canonical strings only for hash-selected
/// candidates; the canonical-string map keeps the original naming order.
fn collect_selected_schema_cores(
    schema: &Value,
    metadata: &SchemaMetadataIndex,
    counts: &std::collections::HashMap<(u128, usize), usize>,
    payloads: &mut BTreeMap<String, RepeatedPayload>,
) {
    let Value::Object(object) = schema else {
        return;
    };
    if let Some(fingerprint) = core_fingerprint(object, metadata)
        && fingerprint.1 >= MIN_SHARED_PROVIDER_PAYLOAD_BYTES
        && counts.contains_key(&fingerprint)
    {
        let core = schema_core(object);
        let key = helm_schema_json_schema_walk::canonical_json_string(&core);
        let payload = payloads.entry(key).or_insert_with(|| RepeatedPayload {
            schema: core,
            uses: 0,
        });
        payload.uses += 1;
    }
    visit_schema_children(object, |child| {
        collect_selected_schema_cores(child, metadata, counts, payloads);
    });
}

fn replace_repeated_schema_cores(
    schema: &mut Value,
    metadata: &SchemaMetadataIndex,
    counts: &std::collections::HashMap<(u128, usize), usize>,
    selected: &BTreeMap<String, (String, Value)>,
    used: &mut BTreeSet<String>,
) {
    let Value::Object(object) = schema else {
        return;
    };
    if let Some(fingerprint) = core_fingerprint(object, metadata)
        && fingerprint.1 >= MIN_SHARED_PROVIDER_PAYLOAD_BYTES
        && counts.contains_key(&fingerprint)
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
        replace_repeated_schema_cores(child, metadata, counts, selected, used);
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
        conditional_schemas: &[LoweredConjunct],
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
            let Some(provider_schema_candidate) = conditional.provider_candidate.as_ref() else {
                continue;
            };
            let target_segments = conditional
                .carrier
                .target_value_path
                .segments()
                .map(helm_schema_core::Segment::encode_component)
                .collect::<Vec<_>>();
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
