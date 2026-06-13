mod merge;
mod path_metadata;
mod path_resolver;
mod path_schema;
pub mod required_inference;
mod resolve_policy;
mod schema_model;
mod schema_tree;
mod use_signals;
mod values_yaml;

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{ChartFacts, ContractProjection};
use helm_schema_k8s::K8sSchemaProvider;

use path_metadata::{PathMetadata, collect_path_metadata};
use path_resolver::PathSchemaResolver;
use schema_tree::{apply_values_descriptions, insert_schema_at_path_segments, object_schema};
use use_signals::{UseSignals, collect_use_signals};

// ---------------------------------------------------------------------------
// Core generation logic
// ---------------------------------------------------------------------------

/// Inputs for JSON Schema generation from the current contract projection.
///
/// The generated schema is derived from template uses plus optional structural
/// signals collected by earlier analysis phases. Values-file descriptions are
/// metadata only: they are applied only to schema nodes that already exist from
/// template or values evidence.
pub struct ValuesSchemaInput<'a> {
    pub contract_projection: &'a ContractProjection,
    pub provider: &'a dyn K8sSchemaProvider,
    pub values_yaml: Option<&'a str>,
    pub type_hints: Option<&'a BTreeMap<String, Vec<Value>>>,
    pub chart_facts: Option<&'a ChartFacts>,
    pub values_descriptions: Option<&'a BTreeMap<String, String>>,
}

impl<'a> ValuesSchemaInput<'a> {
    pub fn new(
        contract_projection: &'a ContractProjection,
        provider: &'a dyn K8sSchemaProvider,
    ) -> Self {
        Self {
            contract_projection,
            provider,
            values_yaml: None,
            type_hints: None,
            chart_facts: None,
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

    pub fn with_chart_facts(mut self, chart_facts: &'a ChartFacts) -> Self {
        self.chart_facts = Some(chart_facts);
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
    let empty_chart_facts = ChartFacts::default();
    let chart_facts = input.chart_facts.unwrap_or(&empty_chart_facts);
    let empty_values_descriptions = BTreeMap::new();
    let values_descriptions = input
        .values_descriptions
        .unwrap_or(&empty_values_descriptions);

    let mut signals = collect_use_signals(input.contract_projection, input.provider);
    signals
        .referenced_value_paths
        .extend(type_hints.keys().cloned());
    let path_metadata =
        collect_path_metadata(input.contract_projection, &signals.referenced_value_paths);
    let mut merged_chart_facts = input.contract_projection.chart_facts();
    merge_chart_facts(&mut merged_chart_facts, chart_facts);

    let values_yaml_doc = input
        .values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);

    let root_schema = build_root_schema(
        signals,
        &path_metadata,
        &values_yaml_doc,
        type_hints,
        &merged_chart_facts,
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

fn merge_chart_facts(dst: &mut ChartFacts, src: &ChartFacts) {
    for (path, fact) in &src.path_facts {
        let entry = dst.path_facts.entry(path.clone()).or_default();
        let had_render_use = entry.has_render_use;
        if fact.has_render_use {
            entry.all_render_uses_self_guarded = if had_render_use {
                entry.all_render_uses_self_guarded && fact.all_render_uses_self_guarded
            } else {
                fact.all_render_uses_self_guarded
            };
        }
        entry.has_render_use |= fact.has_render_use;
        entry.has_fragment_render |= fact.has_fragment_render;
        entry.descendant_accessed |= fact.descendant_accessed;
        entry.has_self_range_guard_render_use |= fact.has_self_range_guard_render_use;
    }
}

#[tracing::instrument(skip_all)]
fn build_root_schema(
    signals: UseSignals,
    path_metadata: &PathMetadata,
    values_yaml_doc: &YamlValue,
    type_hints: &BTreeMap<String, Vec<Value>>,
    chart_facts: &ChartFacts,
    values_descriptions: &BTreeMap<String, String>,
) -> Value {
    let mut root_schema = object_schema(Map::new());
    let path_resolver = PathSchemaResolver::new(
        signals,
        path_metadata,
        values_yaml_doc,
        type_hints,
        chart_facts,
    );

    for resolved_path in path_resolver.resolve_all() {
        insert_schema_at_path_segments(
            &mut root_schema,
            &resolved_path.path_segments,
            resolved_path.schema,
        );
    }

    apply_values_descriptions(&mut root_schema, values_descriptions);

    root_schema
}

#[cfg(test)]
mod tests;
