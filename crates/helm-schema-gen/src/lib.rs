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

use base_schema::{ConditionalTargetIndex, classify_base};
use condition_encoding::{
    HELM_TRUTHY_DEFINITION_NAME, helm_truthy_definition_schema, value_references_helm_truthy,
};
use overlay_lowering::{
    append_conditional_schemas, append_terminal_clauses, collect_conditional_schemas,
};
use path_resolver::PathSchemaResolver;
use provider_definitions::{
    extract_provider_definitions, extract_repeated_provider_payloads, insert_definitions_into_root,
};
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

    let mut values_yaml_doc = input
        .values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);
    values_yaml::apply_values_default_sources(
        &mut values_yaml_doc,
        input.contract_schema_signals.values_default_sources(),
    );

    let root_schema = build_root_schema(
        input.contract_schema_signals,
        &values_yaml_doc,
        values_descriptions,
        input.provider,
    );

    draft07_root_document(root_schema)
}

/// The domain Go's `range` iterates without aborting: collections and nil
/// render; integer counts iterate through Helm's `--set` int64 channel
/// (JSON Schema cannot separate that from the failing values-file float64
/// spelling, so the renderable channel wins) unless the loop body reads
/// member structure integers cannot provide; strings and non-integral
/// numbers fail in every channel.
pub(crate) fn runtime_iterable_schema(allow_integer: bool) -> serde_json::Value {
    let mut arms = vec![
        serde_json::json!({ "type": "array" }),
        serde_json::json!({ "type": "object" }),
    ];
    if allow_integer {
        arms.push(serde_json::json!({ "type": "integer" }));
    }
    arms.push(serde_json::json!({ "type": "null" }));
    serde_json::json!({ "anyOf": arms })
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
    let mut conditional_schemas = collect_conditional_schemas(
        &resolved_paths,
        contract_schema_signals,
        values_yaml_doc,
        provider,
    );
    let provider_definitions = extract_provider_definitions(
        &mut resolved_paths,
        &mut conditional_schemas,
        values_descriptions,
    );
    let conditional_targets = ConditionalTargetIndex::from_conditionals(&conditional_schemas);
    let accepted_values_root_paths = contract_schema_signals
        .schema_evidence_by_value_path()
        .values()
        .filter(|evidence| evidence.facts.accepted_values_root_fragment)
        .map(|evidence| split_value_path(&evidence.value_path))
        .collect::<Vec<_>>();
    let no_owning_ancestors = BTreeSet::new();
    let base_span = tracing::info_span!("base_path_insertion").entered();
    let owning_paths = resolved_paths
        .iter()
        .filter(|resolved_path| {
            classify_base(resolved_path, &conditional_targets, &no_owning_ancestors)
                .owns_descendants()
        })
        .map(|resolved_path| resolved_path.path_segments.clone())
        .collect::<BTreeSet<_>>();
    for resolved_path in &resolved_paths {
        let owner = classify_base(resolved_path, &conditional_targets, &owning_paths);
        let Some(schema) = owner.schema(resolved_path) else {
            continue;
        };
        if owner.replaces() {
            root_schema.replace_path_schema(&resolved_path.path_segments, schema);
        } else {
            root_schema.insert_path_schema(&resolved_path.path_segments, schema);
        }
    }
    drop(base_span);
    append_conditional_schemas(&mut root_schema, conditional_schemas, values_yaml_doc);
    append_terminal_clauses(
        &mut root_schema,
        contract_schema_signals.terminal_clauses(),
        values_yaml_doc,
    );
    // A serialized path's schema is deliberately unconstrained; the
    // declared-default filler must keep the slot present without re-typing
    // it, exactly like a conditional target.
    let mut default_fill_skip_paths = conditional_targets.target_paths.clone();
    for resolved_path in &resolved_paths {
        if resolved_path.used_as_serialized {
            default_fill_skip_paths.insert(resolved_path.path_segments.clone());
        }
    }
    for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        if evidence.facts.used_as_yaml_serialized {
            default_fill_skip_paths.insert(split_value_path(value_path));
        }
    }
    // A directly ranged path accepts the runtime iterable domain, which is
    // wider than any declared default; the filler must not re-type it.
    for value_path in contract_schema_signals.direct_ranged_value_paths() {
        default_fill_skip_paths.insert(split_value_path(value_path));
    }
    let fill_span = tracing::info_span!("default_fill_and_finish").entered();
    {
        let _span = tracing::info_span!("merge_missing_defaults").entered();
        root_schema.merge_missing_values_yaml_defaults_under_roots(
            values_yaml_doc,
            &accepted_values_root_paths,
            &default_fill_skip_paths,
        );
    }
    root_schema.open_helm_global_namespace();

    let mut root_schema = root_schema.into_value();
    if let Ok(declared_defaults) = serde_json::to_value(values_yaml_doc)
        && declared_defaults.is_object()
    {
        let _span = tracing::info_span!("preserve_declared_defaults").entered();
        root_schema =
            resolve_policy::preserve_declared_default_in_schema(root_schema, &declared_defaults);
    }
    let mut provider_definitions = provider_definitions;
    {
        let _span = tracing::info_span!("extract_repeated_provider_payloads").entered();
        provider_definitions.extend(extract_repeated_provider_payloads(&mut root_schema));
    }
    let truthy_span = tracing::info_span!("helm_truthy_scan").entered();
    if value_references_helm_truthy(&root_schema)
        || provider_definitions
            .values()
            .any(value_references_helm_truthy)
    {
        provider_definitions.insert(
            HELM_TRUTHY_DEFINITION_NAME.to_string(),
            helm_truthy_definition_schema(),
        );
    }
    drop(truthy_span);
    insert_definitions_into_root(&mut root_schema, provider_definitions);
    {
        let _span = tracing::info_span!("apply_values_descriptions").entered();
        schema_tree::apply_values_descriptions(&mut root_schema, values_descriptions);
    }
    drop(fill_span);
    root_schema
}

pub(crate) use helm_schema_core::split_value_path;

fn common_prefix_len(left: &[String], right: &[String]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
