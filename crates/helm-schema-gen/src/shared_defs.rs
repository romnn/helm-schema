use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::path_resolver::ResolvedPathSchema;
use crate::schema_model::schema_type;

const DEFINITIONS_KEY: &str = "$defs";
const PROVIDER_DEFINITION_PREFIX: &str = "providerSchema";

/// Provider schema that can be shared if it survives path resolution unchanged.
#[derive(Debug, Clone)]
pub(crate) struct ShareableSchema {
    key: String,
    schema: Value,
}

impl ShareableSchema {
    pub(crate) fn new(schema: Value) -> Self {
        let key = canonical_schema_key(&schema);
        Self { key, schema }
    }

    pub(crate) fn schema(&self) -> &Value {
        &self.schema
    }
}

/// Repeated provider-owned schema leaves that can be emitted as `$defs`.
#[derive(Debug, Default)]
pub(crate) struct SharedSchemaDefinitions {
    definitions_by_name: BTreeMap<String, Value>,
}

impl SharedSchemaDefinitions {
    pub(crate) fn from_resolved_paths(
        resolved_paths: &mut [ResolvedPathSchema],
        values_descriptions: &BTreeMap<String, String>,
    ) -> Self {
        let description_paths = DescriptionPathIndex::new(values_descriptions);
        let entries = SharedSchemaEntries::from_resolved_paths(resolved_paths, &description_paths);
        let mut ref_names_by_key = BTreeMap::new();
        let mut definitions_by_name = BTreeMap::new();
        let mut next_id = 1;

        for (key, entry) in entries.into_repeated_entries() {
            let name = format!("{PROVIDER_DEFINITION_PREFIX}{next_id}");
            next_id += 1;
            ref_names_by_key.insert(key, name.clone());
            definitions_by_name.insert(name, entry.schema);
        }

        for resolved_path in resolved_paths {
            let Some(shareable_schema) = resolved_path.shareable_provider_schema.as_ref() else {
                continue;
            };
            if description_paths.has_description_at_or_below(&resolved_path.path_segments) {
                continue;
            }
            let Some(name) = ref_names_by_key.get(&shareable_schema.key) else {
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
struct SharedSchemaEntries {
    by_key: BTreeMap<String, SharedSchemaEntry>,
}

impl SharedSchemaEntries {
    fn from_resolved_paths(
        resolved_paths: &[ResolvedPathSchema],
        description_paths: &DescriptionPathIndex,
    ) -> Self {
        let mut entries = Self::default();
        for resolved_path in resolved_paths {
            let Some(shareable_schema) = resolved_path.shareable_provider_schema.as_ref() else {
                continue;
            };
            if description_paths.has_description_at_or_below(&resolved_path.path_segments) {
                continue;
            }
            if !is_provider_subtree_schema(shareable_schema.schema()) {
                continue;
            }
            entries.insert(shareable_schema);
        }
        entries
    }

    fn insert(&mut self, shareable_schema: &ShareableSchema) {
        let entry = self
            .by_key
            .entry(shareable_schema.key.clone())
            .or_insert_with(|| SharedSchemaEntry {
                schema: shareable_schema.schema.clone(),
                uses: 0,
            });
        entry.uses += 1;
    }

    fn into_repeated_entries(self) -> impl Iterator<Item = (String, SharedSchemaEntry)> {
        self.by_key.into_iter().filter(|(_, entry)| entry.uses > 1)
    }
}

#[derive(Debug)]
struct SharedSchemaEntry {
    schema: Value,
    uses: usize,
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

fn is_provider_subtree_schema(schema: &Value) -> bool {
    match schema_type(schema) {
        Some("object" | "array") => return true,
        Some(_) => return false,
        None => {}
    }

    let Some(object) = schema.as_object() else {
        return false;
    };
    if object.contains_key("properties")
        || object.contains_key("additionalProperties")
        || object.contains_key("patternProperties")
        || object.contains_key("required")
        || object.contains_key("items")
    {
        return true;
    }

    ["anyOf", "oneOf", "allOf"].into_iter().any(|key| {
        object
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|variants| variants.iter().any(is_provider_subtree_schema))
    })
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

fn canonical_schema_key(schema: &Value) -> String {
    canonicalize_json_value(schema).to_string()
}

fn canonicalize_json_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json_value).collect()),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize_json_value(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn resolved_path(path: &str, schema: Value) -> ResolvedPathSchema {
        ResolvedPathSchema {
            path_segments: path
                .split('.')
                .map(std::string::ToString::to_string)
                .collect(),
            shareable_provider_schema: Some(ShareableSchema::new(schema.clone())),
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
            SharedSchemaDefinitions::from_resolved_paths(&mut paths, &BTreeMap::new());
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
            SharedSchemaDefinitions::from_resolved_paths(&mut paths, &BTreeMap::new());
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

        let definitions = SharedSchemaDefinitions::from_resolved_paths(&mut paths, &descriptions);
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
