mod base_schema;
mod condition_encoding;
mod foreign_schema;
mod merge;
mod overlay_lowering;
mod path_resolver;
mod path_schema;
mod provider_definitions;
mod provider_schema;
pub mod required_inference;
mod resolve_policy;
mod schema_model;
mod schema_node;
mod schema_tree;
mod values_yaml;

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{ContractSchemaSignals, ResourceSchemaOracle};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use base_schema::{BaseInsertionDecision, ConditionalTargetIndex, base_insertion_decision};
use condition_encoding::{
    HELM_TRUTHY_DEFINITION_NAME, helm_truthy_definition_schema, value_references_helm_truthy,
};
use overlay_lowering::{append_conditional_schemas, collect_conditional_schemas};
use path_resolver::PathSchemaResolver;
use provider_definitions::{extract_provider_definitions, insert_definitions_into_root};
use schema_tree::{SchemaDocument, draft07_root_document};

/// Inputs for JSON Schema generation from the current contract schema signals.
///
/// The generated schema is derived from the contract-layer signal bundle plus
/// optional structural signals collected by earlier analysis phases.
/// Values-file descriptions are metadata only: they are applied only to schema
/// nodes that already exist from template or values evidence.
pub struct ValuesSchemaInput<'a> {
    pub contract_schema_signals: &'a ContractSchemaSignals,
    pub provider: &'a dyn ResourceSchemaOracle,
    pub values_yaml: Option<&'a str>,
    pub values_descriptions: Option<&'a BTreeMap<String, String>>,
}

impl<'a> ValuesSchemaInput<'a> {
    pub fn new(
        contract_schema_signals: &'a ContractSchemaSignals,
        provider: &'a dyn ResourceSchemaOracle,
    ) -> Self {
        Self {
            contract_schema_signals,
            provider,
            values_yaml: None,
            values_descriptions: None,
        }
    }

    pub fn with_values_yaml(mut self, values_yaml: Option<&'a str>) -> Self {
        self.values_yaml = values_yaml;
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
    let empty_values_descriptions = BTreeMap::new();
    let values_descriptions = input
        .values_descriptions
        .unwrap_or(&empty_values_descriptions);

    let values_yaml_doc = input
        .values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);

    let root_schema = build_root_schema(
        input.contract_schema_signals,
        &values_yaml_doc,
        values_descriptions,
        input.provider,
    );

    draft07_root_document(root_schema)
}

#[tracing::instrument(skip_all)]
fn build_root_schema(
    contract_schema_signals: &ContractSchemaSignals,
    values_yaml_doc: &YamlValue,
    values_descriptions: &BTreeMap<String, String>,
    provider: &dyn ResourceSchemaOracle,
) -> Value {
    let mut root_schema = SchemaDocument::new_root_object();
    let path_resolver = PathSchemaResolver::new(contract_schema_signals, values_yaml_doc, provider);
    let mut resolved_paths = path_resolver.resolve_all();
    let provider_definitions =
        extract_provider_definitions(&mut resolved_paths, values_descriptions);

    let conditional_schemas = collect_conditional_schemas(
        &resolved_paths,
        contract_schema_signals,
        values_yaml_doc,
        provider,
    );
    let conditional_targets = ConditionalTargetIndex::from_conditionals(&conditional_schemas);
    let accepted_values_root_paths = contract_schema_signals
        .schema_evidence_by_value_path()
        .values()
        .filter(|evidence| evidence.facts.accepted_values_root_fragment)
        .map(|evidence| split_value_path(&evidence.value_path))
        .collect::<Vec<_>>();
    let mut delayed_replacements = Vec::new();
    for resolved_path in &resolved_paths {
        match base_insertion_decision(resolved_path, &conditional_targets) {
            BaseInsertionDecision::Insert(schema) => {
                root_schema.insert_path_schema(&resolved_path.path_segments, schema);
            }
            BaseInsertionDecision::Replace(schema) => {
                delayed_replacements.push((resolved_path.path_segments.clone(), schema));
            }
        }
    }
    // A replaced target under a replaced ancestor adds nothing (the
    // ancestor's base already owns the subtree) and descending into the
    // ancestor's non-object base would coerce it into a closed map.
    let replaced_paths: BTreeSet<Vec<String>> = delayed_replacements
        .iter()
        .map(|(path_segments, _)| path_segments.clone())
        .collect();
    for (path_segments, schema) in delayed_replacements {
        let has_replaced_ancestor = (1..path_segments.len())
            .any(|length| replaced_paths.contains(&path_segments[..length]));
        if !has_replaced_ancestor {
            root_schema.replace_path_schema(&path_segments, schema);
        }
    }

    append_conditional_schemas(&mut root_schema, conditional_schemas, values_yaml_doc);
    root_schema.merge_missing_values_yaml_defaults_under_roots(
        values_yaml_doc,
        &accepted_values_root_paths,
        &conditional_targets.target_paths,
    );

    let mut root_schema = root_schema.into_value();
    let mut provider_definitions = provider_definitions;
    if value_references_helm_truthy(&root_schema) {
        provider_definitions.insert(
            HELM_TRUTHY_DEFINITION_NAME.to_string(),
            helm_truthy_definition_schema(),
        );
    }
    insert_definitions_into_root(&mut root_schema, provider_definitions);
    schema_tree::apply_values_descriptions(&mut root_schema, values_descriptions);
    root_schema
}

pub(crate) fn split_value_path(path: &str) -> Vec<String> {
    path.split('.')
        .filter(|segment| !segment.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

fn common_prefix_len(left: &[String], right: &[String]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
