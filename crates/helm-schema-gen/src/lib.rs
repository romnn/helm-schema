mod merge;
mod path_metadata;
pub mod required_inference;
mod resolve_policy;
mod schema_model;
mod schema_tree;
mod use_signals;
mod values_yaml;

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{ChartFacts, PathFact, ValueUse, derive_chart_facts};
use helm_schema_k8s::{K8sSchemaProvider, type_schema};

use merge::{merge_schema_list, union_schema_list};
use path_metadata::{PathMetadata, collect_path_metadata};
use resolve_policy::{ResolvePolicy, ValuePathSchemaInputs};
use schema_model::{
    add_null_schema, empty_schema, exact_empty_object_schema, is_empty_schema,
    is_fixed_object_schema, is_scalar_like_schema, is_scalar_schema, is_string_like_schema,
    schema_allows_type,
};
use schema_tree::{apply_values_descriptions, insert_schema_at_path_segments, object_schema};
use use_signals::{UseSignals, collect_use_signals};
use values_yaml::{ValuesYamlPathInfo, build_value_path_caches};

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
    mut signals: UseSignals,
    path_metadata: &PathMetadata,
    values_yaml_doc: &YamlValue,
    type_hints: &BTreeMap<String, Vec<Value>>,
    chart_facts: &ChartFacts,
    values_descriptions: &BTreeMap<String, String>,
) -> Value {
    let path_caches = build_value_path_caches(values_yaml_doc, &signals.referenced_value_paths);
    let resolve_policy = ResolvePolicy::default();
    let mut root_schema = object_schema(Map::new());

    for vp in signals.referenced_value_paths {
        let path_segments = path_caches
            .path_segments
            .get(&vp)
            .expect("referenced path must have cached path segments");
        let values_yaml_info = path_caches.values_yaml.get(&vp);
        let path_fact = chart_facts.path_facts.get(&vp).cloned().unwrap_or_default();
        let used_as_fragment = signals.value_paths_used_as_fragment.contains(&vp);
        let is_ranged_source = signals.ranged_value_paths.contains(&vp);
        let provider_schemas = signals
            .provider_schemas_by_value_path
            .remove(&vp)
            .unwrap_or_default();
        let provider_schema = if provider_schemas.len() > 1
            && provider_schemas
                .iter()
                .all(|schema| is_string_like_schema(schema.as_ref()))
        {
            type_schema("string")
        } else {
            merge_schema_list(
                provider_schemas
                    .into_iter()
                    .map(|schema| (*schema).clone())
                    .collect(),
            )
        };
        let metadata_schema = signals
            .metadata_schemas_by_value_path
            .remove(&vp)
            .map_or_else(empty_schema, merge_schema_list);
        let provider_schema = merge_schema_list(vec![provider_schema, metadata_schema]);
        let type_hint_schema = type_hints
            .get(&vp)
            .cloned()
            .map_or_else(empty_schema, merge_schema_list);
        let guard_constraint_schema = signals
            .guard_constraints_by_value_path
            .remove(&vp)
            .map_or_else(empty_schema, merge_schema_list);
        let partial_scalar_schema = if signals.partial_scalar_value_paths.contains(&vp)
            && is_empty_schema(&provider_schema)
            && is_empty_schema(&type_hint_schema)
            && is_empty_schema(&guard_constraint_schema)
            && values_yaml_info.is_none_or(|path_info| is_empty_schema(&path_info.schema))
        {
            type_schema("string")
        } else {
            empty_schema()
        };

        let has_explicit_null_scalar_default = values_yaml_info
            .is_some_and(|path_info| path_info.is_explicit_null)
            && (is_scalar_like_schema(&type_hint_schema)
                || is_scalar_like_schema(&guard_constraint_schema));
        let path_is_nullable = path_metadata.nullable_paths.contains(&vp)
            || type_hints.contains_key(&vp)
            || has_explicit_null_scalar_default;
        let preserve_explicit_null_default = path_is_nullable
            && values_yaml_info.is_some_and(|path_info| path_info.is_explicit_null);
        let preserve_empty_string_fallback = values_yaml_info
            .is_some_and(|path_info| path_info.is_empty_string)
            && ((path_fact.has_render_use && path_fact.all_render_uses_self_guarded)
                || is_scalar_like_schema(&type_hint_schema)
                || is_scalar_like_schema(&guard_constraint_schema));
        let values_yaml_schema = values_yaml_info
            .map(|path_info| {
                values_yaml_schema_for_path(
                    path_info,
                    &path_fact,
                    &provider_schema,
                    used_as_fragment,
                    is_ranged_source,
                )
            })
            .unwrap_or_else(empty_schema);
        let values_yaml_schema = if used_as_fragment && is_empty_schema(&provider_schema) {
            open_fragment_values_schema(values_yaml_schema)
        } else {
            values_yaml_schema
        };
        let values_yaml_schema = if signals.ranged_value_paths.contains(&vp)
            && values_yaml_info.is_some_and(|path_info| path_info.is_mapping)
        {
            generalize_fixed_object_schema_to_open_map(values_yaml_schema)
        } else {
            values_yaml_schema
        };
        let provider_schema = if used_as_fragment
            && is_scalar_schema(&values_yaml_schema)
            && (is_scalar_like_schema(&type_hint_schema)
                || is_scalar_like_schema(&guard_constraint_schema))
        {
            resolve_policy
                .restrict_to_scalar_domain(provider_schema.clone())
                .unwrap_or(provider_schema)
        } else {
            provider_schema
        };

        let merged = resolve_policy.resolve_schema_for_value_path(ValuePathSchemaInputs {
            has_referenced_descendants: path_metadata.paths_with_descendants.contains(&vp),
            used_as_fragment,
            provider_schema,
            values_yaml_schema,
            guard_constraint_schema: merge_schema_list(vec![
                guard_constraint_schema,
                partial_scalar_schema,
            ]),
            type_hint_schema,
            preserve_empty_string_fallback,
        });
        let should_preserve_empty_placeholder = preserve_explicit_empty_placeholder(
            values_yaml_info,
            &path_fact,
            &merged,
            used_as_fragment,
            is_ranged_source,
        );
        let merged = if (preserve_explicit_null_default
            || (is_scalar_like_schema(&merged) && path_metadata.nullable_paths.contains(&vp)))
            && !is_empty_schema(&merged)
        {
            add_null_schema(merged)
        } else if preserve_explicit_null_default {
            type_schema("null")
        } else if should_preserve_empty_placeholder {
            merge_explicit_empty_placeholder(merged, values_yaml_info.expect("placeholder info"))
        } else {
            merged
        };
        insert_schema_at_path_segments(&mut root_schema, path_segments, merged);
    }

    apply_values_descriptions(&mut root_schema, values_descriptions);

    root_schema
}

// ---------------------------------------------------------------------------
// Inference helpers
// ---------------------------------------------------------------------------

fn values_yaml_schema_for_path(
    path_info: &ValuesYamlPathInfo,
    path_fact: &PathFact,
    provider_schema: &Value,
    used_as_fragment: bool,
    is_ranged_source: bool,
) -> Value {
    if path_info.is_empty_map
        && empty_map_placeholder_has_structural_object_use(
            path_fact,
            provider_schema,
            used_as_fragment,
            is_ranged_source,
        )
    {
        return empty_schema();
    }

    path_info.schema.clone()
}

fn preserve_explicit_empty_placeholder(
    path_info: Option<&ValuesYamlPathInfo>,
    path_fact: &PathFact,
    provider_schema: &Value,
    used_as_fragment: bool,
    is_ranged_source: bool,
) -> bool {
    path_info.is_some_and(|info| info.is_empty_map)
        && empty_map_placeholder_has_structural_object_use(
            path_fact,
            provider_schema,
            used_as_fragment,
            is_ranged_source,
        )
}

fn empty_map_placeholder_has_structural_object_use(
    path_fact: &PathFact,
    provider_schema: &Value,
    used_as_fragment: bool,
    is_ranged_source: bool,
) -> bool {
    is_ranged_source
        || path_fact.has_self_range_guard_render_use
        || (schema_allows_type(provider_schema, "object")
            && (used_as_fragment
                || (path_fact.has_render_use && path_fact.all_render_uses_self_guarded)))
}

fn merge_explicit_empty_placeholder(schema: Value, path_info: &ValuesYamlPathInfo) -> Value {
    if path_info.is_empty_map {
        if schema_accepts_empty_object(&schema) {
            return schema;
        }
        union_schema_list(vec![schema, exact_empty_object_schema()])
    } else {
        schema
    }
}

fn schema_accepts_empty_object(schema: &Value) -> bool {
    if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
        return variants.iter().any(schema_accepts_empty_object);
    }

    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        return variants.iter().any(schema_accepts_empty_object);
    }

    if !schema_allows_type(schema, "object") {
        return false;
    }

    let required_is_empty = schema
        .get("required")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty);
    let min_properties_allows_empty = schema
        .get("minProperties")
        .and_then(Value::as_u64)
        .is_none_or(|min| min == 0);

    required_is_empty && min_properties_allows_empty
}

fn generalize_fixed_object_schema_to_open_map(schema: Value) -> Value {
    if !is_fixed_object_schema(&schema) {
        return schema;
    }
    let Some(obj) = schema.as_object() else {
        return schema;
    };
    let Some(properties) = obj.get("properties").and_then(Value::as_object) else {
        return schema;
    };

    let merged_value_schema = merge_schema_list(properties.values().cloned().collect());
    let mut out = obj.clone();
    out.insert("additionalProperties".to_string(), merged_value_schema);
    Value::Object(out)
}

fn open_fragment_values_schema(schema: Value) -> Value {
    open_fragment_values_schema_inner(schema, true)
}

fn open_fragment_values_schema_inner(schema: Value, widen_self: bool) -> Value {
    match schema {
        Value::Object(mut object) => {
            if let Some(Value::Array(variants)) = object.remove("anyOf") {
                object.insert(
                    "anyOf".to_string(),
                    Value::Array(
                        variants
                            .into_iter()
                            .map(|variant| open_fragment_values_schema_inner(variant, widen_self))
                            .collect(),
                    ),
                );
                return Value::Object(object);
            }
            if let Some(Value::Array(variants)) = object.remove("oneOf") {
                object.insert(
                    "oneOf".to_string(),
                    Value::Array(
                        variants
                            .into_iter()
                            .map(|variant| open_fragment_values_schema_inner(variant, widen_self))
                            .collect(),
                    ),
                );
                return Value::Object(object);
            }

            if let Some(items) = object.remove("items") {
                object.insert(
                    "items".to_string(),
                    open_fragment_values_schema_inner(items, false),
                );
            }

            let schema_type = object.get("type").and_then(Value::as_str);
            let is_array = schema_type == Some("array");
            let is_scalar = matches!(
                schema_type,
                Some("boolean" | "integer" | "number" | "string")
            );
            let is_object = schema_type == Some("object");
            if is_object {
                let mut properties = object
                    .remove("properties")
                    .and_then(|value| match value {
                        Value::Object(properties) => Some(properties),
                        _ => None,
                    })
                    .unwrap_or_default();
                for value in properties.values_mut() {
                    *value = open_fragment_values_schema_inner(value.take(), false);
                }
                let additional_properties = if properties.is_empty() {
                    empty_schema()
                } else {
                    merge_schema_list(properties.values().cloned().collect())
                };
                if !properties.is_empty() {
                    object.insert("properties".to_string(), Value::Object(properties));
                }
                object.insert("additionalProperties".to_string(), additional_properties);
            }

            let schema = Value::Object(object);
            if widen_self && is_array {
                union_schema_list(vec![schema, type_schema("null"), type_schema("string")])
            } else if widen_self && is_scalar {
                add_null_schema(schema)
            } else {
                schema
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests;
