use std::collections::{BTreeMap, BTreeSet};

use helm_schema_k8s::{ProviderOrigin, ProviderSchemaSource};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::path_resolver::ResolvedPathSchema;
use crate::provider_schema::ProviderSchemaCandidate;

const DEFINITIONS_KEY: &str = "$defs";
const PROVIDER_DEFINITION_PREFIX: &str = "providerSchema";
const PROVIDER_SOURCE_DEFINITION_PREFIX: &str = "providerSource";

/// Repeated provider-owned schema leaves emitted as root `$defs`.
#[derive(Debug, Default)]
pub(crate) struct ProviderSchemaDefinitions {
    definitions_by_name: BTreeMap<String, Value>,
}

impl ProviderSchemaDefinitions {
    pub(crate) fn from_resolved_paths(
        resolved_paths: &mut [ResolvedPathSchema],
        values_descriptions: &BTreeMap<String, String>,
    ) -> Self {
        let description_paths = DescriptionPathIndex::new(values_descriptions);
        let entries = ProviderSchemaDefinitionEntries::from_resolved_paths(
            resolved_paths,
            &description_paths,
        );
        let mut ref_names_by_key = BTreeMap::new();
        let mut definitions_by_name = BTreeMap::new();
        let mut used_definition_names = BTreeSet::new();
        let mut next_id = 1;

        for (key, entry) in entries.into_repeated_entries() {
            let name = next_definition_name(&entry, &mut used_definition_names, &mut next_id);
            ref_names_by_key.insert(key, name.clone());
            definitions_by_name.insert(name, entry.schema);
        }

        for resolved_path in resolved_paths {
            let Some(provider_schema_candidate) = resolved_path.provider_schema_candidate.as_ref()
            else {
                continue;
            };
            if description_paths.has_description_at_or_below(&resolved_path.path_segments) {
                continue;
            }
            let Some(name) = ref_names_by_key.get(provider_schema_candidate.key()) else {
                continue;
            };
            resolved_path.schema = reference_schema(name);
        }

        Self {
            definitions_by_name,
        }
    }

    pub(crate) fn insert_into_root(self, schema: &mut Value) {
        if self.definitions_by_name.is_empty() {
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

        for (name, definition) in self.definitions_by_name {
            definitions.insert(name, definition);
        }
    }
}

#[derive(Debug, Default)]
struct ProviderSchemaDefinitionEntries {
    by_key: BTreeMap<String, ProviderSchemaDefinitionEntry>,
}

impl ProviderSchemaDefinitionEntries {
    fn from_resolved_paths(
        resolved_paths: &[ResolvedPathSchema],
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
                source_definition_names_by_identity: BTreeMap::new(),
                uses: 0,
            });
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
    source_definition_names_by_identity: BTreeMap<ProviderSourceIdentity, String>,
    uses: usize,
}

impl ProviderSchemaDefinitionEntry {
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
            .map(|(path, _)| {
                path.split('.')
                    .filter(|segment| !segment.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect()
            })
            .collect();
        Self { paths }
    }

    fn has_description_at_or_below(&self, path_segments: &[String]) -> bool {
        self.paths
            .iter()
            .any(|description_path| path_segments_are_prefix(path_segments, description_path))
    }
}

fn path_segments_are_prefix(prefix: &[String], path: &[String]) -> bool {
    prefix.len() <= path.len() && prefix.iter().zip(path).all(|(left, right)| left == right)
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

    let hex = format!("{:x}", hasher.finalize());
    hex.chars().take(12).collect()
}

#[cfg(test)]
mod tests {
    use helm_schema_k8s::{ProviderOrigin, ProviderSchemaFragment, ProviderSchemaSource};
    use serde_json::json;

    use super::*;

    fn resolved_path(path: &str, schema: Value) -> ResolvedPathSchema {
        ResolvedPathSchema {
            path_segments: path
                .split('.')
                .map(std::string::ToString::to_string)
                .collect(),
            provider_schema_candidate: Some(ProviderSchemaCandidate::new(schema.clone())),
            schema,
        }
    }

    fn k8s_source(pointer: &str) -> ProviderSchemaSource {
        ProviderSchemaSource::kubernetes_openapi(
            "default",
            "v1.35.0",
            "io.k8s.api.core.v1.Pod.json",
            pointer,
        )
    }

    fn sourced_provider_schema_candidate(schema: Value, pointer: &str) -> ProviderSchemaCandidate {
        ProviderSchemaCandidate::from_provider_fragment(
            ProviderSchemaFragment::new(schema).with_source(k8s_source(pointer)),
        )
    }

    fn resolved_sourced_path(path: &str, schema: Value, pointer: &str) -> ResolvedPathSchema {
        ResolvedPathSchema {
            path_segments: path
                .split('.')
                .map(std::string::ToString::to_string)
                .collect(),
            provider_schema_candidate: Some(sourced_provider_schema_candidate(
                schema.clone(),
                pointer,
            )),
            schema,
        }
    }

    #[test]
    fn repeated_provider_subtrees_move_to_root_definitions() {
        let provider_schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false
        });
        let mut paths = vec![
            resolved_path("first", provider_schema.clone()),
            resolved_path("second", provider_schema.clone()),
        ];

        let definitions =
            ProviderSchemaDefinitions::from_resolved_paths(&mut paths, &BTreeMap::new());
        let mut root = json!({ "type": "object", "properties": {} });
        definitions.insert_into_root(&mut root);

        assert_eq!(
            paths[0].schema,
            json!({ "$ref": "#/$defs/providerSchema1" })
        );
        assert_eq!(
            paths[1].schema,
            json!({ "$ref": "#/$defs/providerSchema1" })
        );
        assert_eq!(
            root.pointer("/$defs/providerSchema1"),
            Some(&provider_schema)
        );
    }

    #[test]
    fn provider_fragment_source_survives_candidate_lowering() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        });
        let candidate = sourced_provider_schema_candidate(schema, "/definitions/Metadata");
        let source = candidate.source().expect("provider source should survive");

        assert_eq!(source.origin(), ProviderOrigin::KubernetesOpenApi);
        assert_eq!(source.source_id(), "default");
        assert_eq!(source.version(), Some("v1.35.0"));
        assert_eq!(source.filename(), "io.k8s.api.core.v1.Pod.json");
        assert_eq!(source.pointer(), "/definitions/Metadata");
    }

    #[test]
    fn repeated_provider_subtrees_with_one_source_use_source_stable_definition_name() {
        let provider_schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false
        });
        let source = k8s_source("/definitions/Metadata");
        let definition_name = source_definition_name(&source);
        let mut paths = vec![
            resolved_sourced_path("first", provider_schema.clone(), source.pointer()),
            resolved_sourced_path("second", provider_schema.clone(), source.pointer()),
        ];

        let definitions =
            ProviderSchemaDefinitions::from_resolved_paths(&mut paths, &BTreeMap::new());
        let mut root = json!({ "type": "object", "properties": {} });
        definitions.insert_into_root(&mut root);

        assert_eq!(
            paths[0].schema,
            json!({ "$ref": format!("#/$defs/{definition_name}") })
        );
        assert_eq!(
            paths[1].schema,
            json!({ "$ref": format!("#/$defs/{definition_name}") })
        );
        assert_eq!(
            root.pointer(&format!("/$defs/{definition_name}")),
            Some(&provider_schema)
        );
    }

    #[test]
    fn structurally_equal_provider_schemas_share_even_with_different_sources() {
        let provider_schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false
        });
        let mut paths = vec![
            ResolvedPathSchema {
                path_segments: vec!["first".to_string()],
                provider_schema_candidate: Some(sourced_provider_schema_candidate(
                    provider_schema.clone(),
                    "/definitions/First",
                )),
                schema: provider_schema.clone(),
            },
            ResolvedPathSchema {
                path_segments: vec!["second".to_string()],
                provider_schema_candidate: Some(sourced_provider_schema_candidate(
                    provider_schema.clone(),
                    "/definitions/Second",
                )),
                schema: provider_schema.clone(),
            },
        ];

        let definitions =
            ProviderSchemaDefinitions::from_resolved_paths(&mut paths, &BTreeMap::new());
        let mut root = json!({ "type": "object", "properties": {} });
        definitions.insert_into_root(&mut root);

        assert_eq!(
            paths[0].schema,
            json!({ "$ref": "#/$defs/providerSchema1" })
        );
        assert_eq!(
            paths[1].schema,
            json!({ "$ref": "#/$defs/providerSchema1" })
        );
        assert_eq!(
            root.pointer("/$defs/providerSchema1"),
            Some(&provider_schema)
        );
    }

    #[test]
    fn scalar_provider_schemas_stay_inline() {
        let provider_schema = json!({ "type": "string" });
        let mut paths = vec![
            resolved_path("first", provider_schema.clone()),
            resolved_path("second", provider_schema.clone()),
        ];

        let definitions =
            ProviderSchemaDefinitions::from_resolved_paths(&mut paths, &BTreeMap::new());
        let mut root = json!({ "type": "object", "properties": {} });
        definitions.insert_into_root(&mut root);

        assert_eq!(paths[0].schema, provider_schema);
        assert!(root.pointer("/$defs").is_none());
    }

    #[test]
    fn described_provider_subtrees_stay_inline_even_when_other_paths_share_definition() {
        let provider_schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false
        });
        let mut paths = vec![
            resolved_path("first", provider_schema.clone()),
            resolved_path("second", provider_schema.clone()),
            resolved_path("third", provider_schema.clone()),
        ];
        let descriptions =
            BTreeMap::from([("first.name".to_string(), "chart-authored name".to_string())]);

        let definitions = ProviderSchemaDefinitions::from_resolved_paths(&mut paths, &descriptions);
        let mut root = json!({ "type": "object", "properties": {} });
        definitions.insert_into_root(&mut root);

        assert_eq!(paths[0].schema, provider_schema);
        assert_eq!(
            paths[1].schema,
            json!({ "$ref": "#/$defs/providerSchema1" })
        );
        assert_eq!(
            paths[2].schema,
            json!({ "$ref": "#/$defs/providerSchema1" })
        );
        assert_eq!(
            root.pointer("/$defs/providerSchema1"),
            Some(&provider_schema)
        );
    }
}
