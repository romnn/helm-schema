mod merge;
mod path_resolver;
mod path_schema;
mod provider_schema;
pub mod required_inference;
mod resolve_policy;
mod schema_model;
mod schema_tree;
mod shared_defs;
mod use_signals;
mod values_yaml;

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{ContractSchemaSignals, ContractValuePathFacts};
use helm_schema_k8s::K8sSchemaProvider;

use path_resolver::PathSchemaResolver;
use schema_tree::{apply_values_descriptions, insert_schema_at_path_segments, object_schema};
use shared_defs::SharedSchemaDefinitions;
use use_signals::{UseSignals, collect_use_signals};

// ---------------------------------------------------------------------------
// Core generation logic
// ---------------------------------------------------------------------------

/// Inputs for JSON Schema generation from the current contract schema signals.
///
/// The generated schema is derived from the contract-layer signal bundle plus
/// optional structural signals collected by earlier analysis phases.
/// Values-file descriptions are metadata only: they are applied only to schema
/// nodes that already exist from template or values evidence.
pub struct ValuesSchemaInput<'a> {
    pub contract_schema_signals: &'a ContractSchemaSignals,
    pub provider: &'a dyn K8sSchemaProvider,
    pub values_yaml: Option<&'a str>,
    pub type_hints: Option<&'a BTreeMap<String, Vec<Value>>>,
    pub values_descriptions: Option<&'a BTreeMap<String, String>>,
}

impl<'a> ValuesSchemaInput<'a> {
    pub fn new(
        contract_schema_signals: &'a ContractSchemaSignals,
        provider: &'a dyn K8sSchemaProvider,
    ) -> Self {
        Self {
            contract_schema_signals,
            provider,
            values_yaml: None,
            type_hints: None,
            values_descriptions: None,
        }
    }

    pub fn with_values_yaml(mut self, values_yaml: Option<&'a str>) -> Self {
        self.values_yaml = values_yaml;
        self
    }

    pub fn with_type_hints(mut self, type_hints: &'a BTreeMap<String, Vec<Value>>) -> Self {
        self.type_hints = Some(type_hints);
        self
    }

    pub fn with_values_descriptions(
        mut self,
        values_descriptions: &'a BTreeMap<String, String>,
    ) -> Self {
        self.values_descriptions = Some(values_descriptions);
        self
    }
}

/// Generate a JSON Schema with chart-authored values-file descriptions.
///
/// The output schema has no `required` arrays inferred by helm-schema; callers
/// that want that behaviour layer [`required_inference::apply_required_inference`]
/// on top of the returned schema. Keeping required-inference outside this
/// function isolates a heuristic feature from the core schema-generation
/// pipeline.
#[tracing::instrument(skip_all)]
pub fn generate_values_schema(input: ValuesSchemaInput<'_>) -> Value {
    let empty_type_hints = BTreeMap::new();
    let type_hints = input.type_hints.unwrap_or(&empty_type_hints);
    let empty_values_descriptions = BTreeMap::new();
    let values_descriptions = input
        .values_descriptions
        .unwrap_or(&empty_values_descriptions);

    let path_signals = input.contract_schema_signals.path_signals.clone();
    let mut value_path_facts = input.contract_schema_signals.value_path_facts.clone();
    let mut signals = collect_use_signals(
        path_signals,
        &input.contract_schema_signals.provider_schema_uses,
        input.provider,
    );
    signals
        .referenced_value_paths
        .extend(type_hints.keys().cloned());
    mark_type_hint_descendant_facts(&mut value_path_facts, type_hints.keys());

    let values_yaml_doc = input
        .values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);

    let root_schema = build_root_schema(
        signals,
        &value_path_facts,
        &values_yaml_doc,
        type_hints,
        values_descriptions,
    );

    let mut out = Map::new();
    out.insert(
        "$schema".to_string(),
        Value::String("http://json-schema.org/draft-07/schema#".to_string()),
    );

    if let Value::Object(obj) = root_schema {
        for (k, v) in obj {
            out.insert(k, v);
        }
    } else {
        out.insert("type".to_string(), Value::String("object".to_string()));
        out.insert("properties".to_string(), Value::Object(Map::new()));
        out.insert("additionalProperties".to_string(), Value::Bool(false));
    }
    Value::Object(out)
}

fn mark_type_hint_descendant_facts<'a>(
    value_path_facts: &mut BTreeMap<String, ContractValuePathFacts>,
    paths: impl IntoIterator<Item = &'a String>,
) {
    for path in paths {
        let mut segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            value_path_facts
                .entry(segments.join("."))
                .or_default()
                .has_referenced_descendants = true;
        }
    }
}

#[tracing::instrument(skip_all)]
fn build_root_schema(
    signals: UseSignals,
    value_path_facts: &BTreeMap<String, ContractValuePathFacts>,
    values_yaml_doc: &YamlValue,
    type_hints: &BTreeMap<String, Vec<Value>>,
    values_descriptions: &BTreeMap<String, String>,
) -> Value {
    let mut root_schema = object_schema(Map::new());
    let path_resolver =
        PathSchemaResolver::new(signals, value_path_facts, values_yaml_doc, type_hints);
    let mut resolved_paths = path_resolver.resolve_all();
    let shared_definitions =
        SharedSchemaDefinitions::from_resolved_paths(&mut resolved_paths, values_descriptions);

    for resolved_path in resolved_paths {
        insert_schema_at_path_segments(
            &mut root_schema,
            &resolved_path.path_segments,
            resolved_path.schema,
        );
    }

    shared_definitions.insert_into_root(&mut root_schema);
    apply_values_descriptions(&mut root_schema, values_descriptions);

    root_schema
}

#[cfg(test)]
mod tests;
