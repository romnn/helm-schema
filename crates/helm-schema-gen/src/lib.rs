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

use helm_schema_ir::{ChartFacts, ValueUse, derive_chart_facts};
use helm_schema_k8s::K8sSchemaProvider;

use path_metadata::{PathMetadata, collect_path_metadata};
use path_resolver::PathSchemaResolver;
use schema_tree::{apply_values_descriptions, insert_schema_at_path_segments, object_schema};
use use_signals::{UseSignals, collect_use_signals};

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Generates a JSON Schema for Helm `values.yaml` from IR and a K8s schema provider.
pub trait ValuesSchemaGenerator {
    fn generate(&self, uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> Value;
}

// ---------------------------------------------------------------------------
// Default implementation
// ---------------------------------------------------------------------------

/// Default values schema generator.
///
/// Collects all `.Values.*` uses, infers their types from the K8s schema
/// provider and template-derived facts, merges conflicting schemas, and builds
/// a nested JSON Schema tree.
pub struct DefaultValuesSchemaGenerator;

impl ValuesSchemaGenerator for DefaultValuesSchemaGenerator {
    fn generate(&self, uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> Value {
        generate_values_schema(uses, provider)
    }
}

// ---------------------------------------------------------------------------
// Core generation logic
// ---------------------------------------------------------------------------

pub fn generate_values_schema(uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> Value {
    generate_values_schema_with_values_yaml(uses, provider, None)
}

/// Generate a JSON Schema for Helm values.
///
/// If `values_yaml` is provided, it is used as an additional type signal:
/// scalar types found in `values.yaml` may be preferred over provider-derived
/// schemas in some cases (e.g. when values are scalar presets expanded into
/// full objects by templates).
pub fn generate_values_schema_with_values_yaml(
    uses: &[ValueUse],
    provider: &dyn K8sSchemaProvider,
    values_yaml: Option<&str>,
) -> Value {
    generate_values_schema_full(uses, provider, values_yaml, &BTreeMap::new())
}

/// Generate a JSON Schema with all available signals.
///
/// Extends [`generate_values_schema_with_values_yaml`] with one extra
/// input:
///
/// - `type_hints`: per-path JSON Schema fragments inferred from `default
///   <literal> .Values.X` patterns in templates (see
///   [`helm_schema_ir::extract_default_type_hints`]). When values.yaml ships a
///   null default for a hinted path, the null is preserved alongside the
///   hint, producing a nullable union. Literal-only because we need the
///   type to build the schema fragment.
///
/// The output schema has no `required` arrays inferred by helm-schema;
/// callers that want that behaviour layer
/// [`required_inference::apply_required_inference`] on top of the
/// returned schema. Keeping required-inference outside this function
/// isolates a heuristic feature from the core schema-generation
/// pipeline.
#[tracing::instrument(skip_all)]
pub fn generate_values_schema_full(
    uses: &[ValueUse],
    provider: &dyn K8sSchemaProvider,
    values_yaml: Option<&str>,
    type_hints: &BTreeMap<String, Vec<Value>>,
) -> Value {
    let chart_facts = ChartFacts::default();
    generate_values_schema_full_with_facts(uses, provider, values_yaml, type_hints, &chart_facts)
}

#[tracing::instrument(skip_all)]
pub fn generate_values_schema_full_with_facts(
    uses: &[ValueUse],
    provider: &dyn K8sSchemaProvider,
    values_yaml: Option<&str>,
    type_hints: &BTreeMap<String, Vec<Value>>,
    chart_facts: &ChartFacts,
) -> Value {
    generate_values_schema_full_with_facts_and_descriptions(
        uses,
        provider,
        values_yaml,
        type_hints,
        chart_facts,
        &BTreeMap::new(),
    )
}

/// Generate a JSON Schema with chart-authored values-file descriptions.
///
/// `values_descriptions` is metadata only. A description is applied only when
/// the schema node already exists from template or values evidence, so comments
/// cannot create accepted values paths or influence inferred types.
#[tracing::instrument(skip_all)]
pub fn generate_values_schema_full_with_facts_and_descriptions(
    uses: &[ValueUse],
    provider: &dyn K8sSchemaProvider,
    values_yaml: Option<&str>,
    type_hints: &BTreeMap<String, Vec<Value>>,
    chart_facts: &ChartFacts,
    values_descriptions: &BTreeMap<String, String>,
) -> Value {
    let mut signals = collect_use_signals(uses, provider);
    signals
        .referenced_value_paths
        .extend(type_hints.keys().cloned());
    let path_metadata = collect_path_metadata(uses, &signals.referenced_value_paths);
    let mut merged_chart_facts = derive_chart_facts(uses);
    merge_chart_facts(&mut merged_chart_facts, chart_facts);

    let values_yaml_doc = values_yaml
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
