mod merge;
pub mod required_inference;
mod resolve_policy;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{ChartFacts, Guard, PathFact, ValueKind, ValueUse, derive_chart_facts};
use helm_schema_k8s::{K8sSchemaProvider, type_schema};

use merge::{merge_schema_list, merge_two_schemas, union_schema_list};
use resolve_policy::{ResolvePolicy, ValuePathSchemaInputs};

struct UseSignals {
    referenced_value_paths: BTreeSet<String>,
    ranged_value_paths: BTreeSet<String>,
    value_paths_used_as_fragment: BTreeSet<String>,
    partial_scalar_value_paths: BTreeSet<String>,
    provider_schemas_by_value_path: BTreeMap<String, Vec<Arc<Value>>>,
    metadata_schemas_by_value_path: BTreeMap<String, Vec<Value>>,
    guard_constraints_by_value_path: BTreeMap<String, Vec<Value>>,
}

struct PathMetadata {
    nullable_paths: BTreeSet<String>,
    paths_with_descendants: BTreeSet<String>,
}

struct ValuesYamlPathInfo {
    schema: Value,
    is_explicit_null: bool,
    is_empty_string: bool,
    is_empty_map: bool,
    is_mapping: bool,
}

struct ValuePathCaches {
    path_segments: BTreeMap<String, Vec<String>>,
    values_yaml: BTreeMap<String, ValuesYamlPathInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProviderSchemaLookupKey {
    resource: helm_schema_ir::ResourceRef,
    path: helm_schema_ir::YamlPath,
    kind: ValueKind,
}

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

#[tracing::instrument(skip_all, fields(uses = uses.len()))]
fn collect_use_signals(uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> UseSignals {
    let mut referenced_value_paths: BTreeSet<String> = BTreeSet::new();
    let mut ranged_value_paths: BTreeSet<String> = BTreeSet::new();
    let mut value_paths_used_as_fragment: BTreeSet<String> = BTreeSet::new();
    let mut partial_scalar_value_paths: BTreeSet<String> = BTreeSet::new();
    let mut provider_schemas_by_value_path: BTreeMap<String, Vec<Arc<Value>>> = BTreeMap::new();
    let mut metadata_schemas_by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut guard_constraints_by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut provider_schema_cache: HashMap<ProviderSchemaLookupKey, Option<Arc<Value>>> =
        HashMap::new();
    let resolve_policy = ResolvePolicy::default();

    for u in uses {
        if u.source_expr.trim().is_empty() {
            continue;
        }

        referenced_value_paths.insert(u.source_expr.clone());
        if u.kind == ValueKind::Fragment {
            value_paths_used_as_fragment.insert(u.source_expr.clone());
        }
        if u.kind == ValueKind::PartialScalar && !u.path.0.is_empty() {
            partial_scalar_value_paths.insert(u.source_expr.clone());
        }
        for g in &u.guards {
            for path in g.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                referenced_value_paths.insert(path.to_string());
                if matches!(g, Guard::Range { .. }) {
                    ranged_value_paths.insert(path.to_string());
                }

                if let Some(schema) = resolve_policy.guard_constraint_schema(g) {
                    guard_constraints_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(schema);
                }
            }
        }

        if u.kind != ValueKind::PartialScalar
            && !u.path.0.is_empty()
            && let Some(resource) = &u.resource
        {
            let lookup_key = ProviderSchemaLookupKey {
                resource: resource.clone(),
                path: u.path.clone(),
                kind: u.kind,
            };
            let schema = match provider_schema_cache.entry(lookup_key) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.get().clone(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let schema = lookup_provider_schema(provider, u, &resolve_policy);
                    entry.insert(schema.clone());
                    schema
                }
            };
            if let Some(schema) = schema {
                let provider_schemas = provider_schemas_by_value_path
                    .entry(u.source_expr.clone())
                    .or_default();
                if !provider_schemas
                    .iter()
                    .any(|existing| Arc::ptr_eq(existing, &schema))
                {
                    provider_schemas.push(schema);
                }
            }
        }

        if let Some(schema) = infer_metadata_path_schema(&u.path.0) {
            metadata_schemas_by_value_path
                .entry(u.source_expr.clone())
                .or_default()
                .push(schema);
        }
    }

    UseSignals {
        referenced_value_paths,
        ranged_value_paths,
        value_paths_used_as_fragment,
        partial_scalar_value_paths,
        provider_schemas_by_value_path,
        metadata_schemas_by_value_path,
        guard_constraints_by_value_path,
    }
}

#[tracing::instrument(
    skip_all,
    fields(
        resource_kind = use_
            .resource
            .as_ref()
            .map(|resource| resource.kind.as_str())
            .unwrap_or(""),
        resource_api_version = use_
            .resource
            .as_ref()
            .map(|resource| resource.api_version.as_str())
            .unwrap_or(""),
        path_len = use_.path.0.len(),
    )
)]
fn lookup_provider_schema(
    provider: &dyn K8sSchemaProvider,
    use_: &ValueUse,
    resolve_policy: &ResolvePolicy,
) -> Option<Arc<Value>> {
    provider
        .schema_for_use(use_)
        .and_then(|schema| resolve_policy.provider_schema_for_value_use(schema, use_))
        .map(Arc::new)
}

#[tracing::instrument(skip_all)]
fn collect_path_metadata(
    uses: &[ValueUse],
    referenced_value_paths: &BTreeSet<String>,
) -> PathMetadata {
    let resolve_policy = ResolvePolicy::default();
    PathMetadata {
        nullable_paths: resolve_policy.nullable_value_paths(uses),
        paths_with_descendants: collect_paths_with_descendants(referenced_value_paths),
    }
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

fn schema_type(v: &Value) -> Option<&str> {
    v.as_object()?.get("type")?.as_str()
}

fn is_scalar_schema(v: &Value) -> bool {
    matches!(
        schema_type(v),
        Some("string" | "integer" | "number" | "boolean")
    )
}

fn is_string_like_schema(v: &Value) -> bool {
    if schema_type(v) == Some("string") {
        return true;
    }

    let Some(obj) = v.as_object() else {
        return false;
    };

    if let Some(Value::Array(values)) = obj.get("enum") {
        return !values.is_empty()
            && values
                .iter()
                .all(|value| matches!(value, Value::String(_) | Value::Null));
    }

    if let Some(Value::Array(types)) = obj.get("type") {
        return types
            .iter()
            .all(|value| matches!(value.as_str(), Some("string" | "null")));
    }

    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(variants)) = obj.get(key) {
            return !variants.is_empty() && variants.iter().all(is_string_like_schema);
        }
    }

    false
}

fn is_scalar_like_schema(v: &Value) -> bool {
    if is_scalar_schema(v) {
        return true;
    }

    let Some(obj) = v.as_object() else {
        return false;
    };

    if let Some(Value::Array(values)) = obj.get("enum") {
        return !values.is_empty()
            && values.iter().all(|value| {
                matches!(
                    value,
                    Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
                )
            });
    }

    if let Some(Value::Array(types)) = obj.get("type") {
        return types.iter().all(|value| {
            matches!(
                value.as_str(),
                Some("string" | "number" | "integer" | "boolean" | "null")
            )
        });
    }

    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(variants)) = obj.get(key) {
            return !variants.is_empty() && variants.iter().all(is_scalar_like_schema);
        }
    }

    false
}

fn infer_metadata_path_schema(path: &[String]) -> Option<Value> {
    let last = path.last()?.as_str();
    let prev = path.get(path.len().checked_sub(2)?)?.as_str();
    if prev != "metadata" {
        return None;
    }

    match last {
        "labels" | "annotations" => Some(string_map_schema()),
        "name" | "namespace" => Some(type_schema("string")),
        _ => None,
    }
}

fn string_map_schema() -> Value {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("additionalProperties".to_string(), type_schema("string"));
    Value::Object(schema)
}

fn is_object_or_array_schema(v: &Value) -> bool {
    matches!(schema_type(v), Some("object" | "array"))
}

fn is_fixed_object_schema(v: &Value) -> bool {
    if schema_type(v) != Some("object") {
        return false;
    }
    let Some(obj) = v.as_object() else {
        return false;
    };
    obj.get("properties")
        .and_then(Value::as_object)
        .is_some_and(|props| !props.is_empty())
        && obj.get("additionalProperties") == Some(&Value::Bool(false))
}

fn is_open_string_map_schema(v: &Value) -> bool {
    if schema_type(v) != Some("object") {
        return false;
    }
    let Some(obj) = v.as_object() else {
        return false;
    };
    matches!(
        obj.get("additionalProperties"),
        Some(Value::Object(map))
            if map.get("type").and_then(Value::as_str) == Some("string")
    )
}

fn schema_allows_scalar_type(schema: &Value, scalar_ty: &str) -> bool {
    if let Some(ty) = schema_type(schema) {
        return ty == scalar_ty;
    }

    let Some(obj) = schema.as_object() else {
        return false;
    };

    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Array(variants)) = obj.get(key)
            && variants
                .iter()
                .any(|v| schema_allows_scalar_type(v, scalar_ty))
        {
            return true;
        }
    }

    false
}

fn schema_allows_type(schema: &Value, expected_ty: &str) -> bool {
    if let Some(ty) = schema_type(schema) {
        return ty == expected_ty;
    }

    let Some(obj) = schema.as_object() else {
        return false;
    };

    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Array(variants)) = obj.get(key)
            && variants
                .iter()
                .any(|variant| schema_allows_type(variant, expected_ty))
        {
            return true;
        }
    }

    false
}

fn add_null_schema(schema: Value) -> Value {
    if schema.get("anyOf").and_then(Value::as_array).is_some()
        || schema.get("oneOf").and_then(Value::as_array).is_some()
    {
        union_schema_list(vec![schema, type_schema("null")])
    } else {
        merge_two_schemas(schema, type_schema("null"))
    }
}

fn empty_string_schema() -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("string".to_string())),
            (
                "enum".to_string(),
                Value::Array(vec![Value::String(String::new())]),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn schema_permits_empty_string(schema: &Value) -> bool {
    if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
        return variants.iter().any(schema_permits_empty_string);
    }
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        return variants.iter().any(schema_permits_empty_string);
    }

    let Some(obj) = schema.as_object() else {
        return false;
    };
    if let Some(values) = obj.get("enum").and_then(Value::as_array) {
        return values.iter().any(|value| value.as_str() == Some(""));
    }
    if obj.get("pattern").is_some() {
        return false;
    }

    let type_allows_string = obj.get("type").and_then(Value::as_str) == Some("string")
        || obj
            .get("type")
            .and_then(Value::as_array)
            .is_some_and(|types| types.iter().any(|value| value.as_str() == Some("string")));
    type_allows_string
        && obj
            .get("minLength")
            .and_then(Value::as_u64)
            .is_none_or(|min_length| min_length == 0)
}

fn is_empty_schema(v: &Value) -> bool {
    v.as_object().is_some_and(serde_json::Map::is_empty)
}

fn empty_schema() -> Value {
    Value::Object(Map::new())
}

#[tracing::instrument(skip_all)]
fn build_value_path_caches(
    values_yaml_doc: &YamlValue,
    referenced_value_paths: &BTreeSet<String>,
) -> ValuePathCaches {
    let path_segments: BTreeMap<String, Vec<String>> = referenced_value_paths
        .iter()
        .map(|path| {
            (
                path.clone(),
                path.split('.')
                    .filter(|segment| !segment.is_empty())
                    .map(std::string::ToString::to_string)
                    .collect(),
            )
        })
        .collect();

    let values_yaml = path_segments
        .iter()
        .filter_map(|(path, segments)| {
            lookup_values_yaml_path_info(values_yaml_doc, segments)
                .map(|path_info| (path.clone(), path_info))
        })
        .collect();

    ValuePathCaches {
        path_segments,
        values_yaml,
    }
}

fn lookup_values_yaml_path_info(
    doc: &YamlValue,
    path_segments: &[String],
) -> Option<ValuesYamlPathInfo> {
    if path_segments.is_empty() {
        return None;
    }

    let values = lookup_values_yaml_values(doc, path_segments)?;
    if values.is_empty() {
        return None;
    }

    let schema = merge_schema_list(values.iter().copied().map(schema_from_yaml_value).collect());
    let is_explicit_null = values.len() == 1 && matches!(values[0], YamlValue::Null);
    let is_empty_string = values
        .iter()
        .any(|value| matches!(value, YamlValue::String(value) if value.is_empty()));
    let is_empty_map = values
        .iter()
        .all(|value| matches!(value, YamlValue::Mapping(map) if map.is_empty()));
    let is_mapping = values
        .iter()
        .all(|value| matches!(value, YamlValue::Mapping(_)));

    Some(ValuesYamlPathInfo {
        schema,
        is_explicit_null,
        is_empty_string,
        is_empty_map,
        is_mapping,
    })
}

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

fn collect_paths_with_descendants(paths: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for path in paths {
        let mut segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            out.insert(segments.join("."));
        }
    }
    out
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

fn exact_empty_object_schema() -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            ("properties".to_string(), Value::Object(Map::new())),
            ("additionalProperties".to_string(), Value::Bool(false)),
            ("maxProperties".to_string(), Value::Number(0.into())),
        ]
        .into_iter()
        .collect(),
    )
}

fn lookup_values_yaml_values<'a>(
    doc: &'a YamlValue,
    path_segments: &[String],
) -> Option<Vec<&'a YamlValue>> {
    if path_segments.is_empty() {
        return Some(vec![doc]);
    }

    let head = path_segments[0].as_str();
    let tail = &path_segments[1..];

    match doc {
        YamlValue::Mapping(m) => {
            let k = YamlValue::String(head.to_string());
            let next = m.get(&k)?;
            lookup_values_yaml_values(next, tail)
        }
        YamlValue::Sequence(seq) if head == "*" => {
            let mut out: Vec<&'a YamlValue> = Vec::new();
            for it in seq {
                if let Some(mut child) = lookup_values_yaml_values(it, tail) {
                    out.append(&mut child);
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

fn schema_from_yaml_value(v: &YamlValue) -> Value {
    match v {
        YamlValue::Null | YamlValue::Tagged(_) => empty_schema(),
        YamlValue::Bool(_) => type_schema("boolean"),
        YamlValue::Number(n) => {
            if n.as_i64().is_some() || n.as_u64().is_some() {
                type_schema("integer")
            } else {
                type_schema("number")
            }
        }
        YamlValue::String(_) => type_schema("string"),
        YamlValue::Sequence(seq) => {
            let items = if seq.is_empty() {
                empty_schema()
            } else {
                merge_schema_list(seq.iter().map(schema_from_yaml_value).collect())
            };
            Value::Object(
                [
                    ("type".to_string(), Value::String("array".to_string())),
                    ("items".to_string(), items),
                ]
                .into_iter()
                .collect(),
            )
        }
        YamlValue::Mapping(m) => {
            if m.is_empty() {
                return unknown_object_schema();
            }
            let mut props = Map::new();
            for (k, v) in m {
                let Some(key) = k.as_str() else {
                    continue;
                };
                props.insert(key.to_string(), schema_from_yaml_value(v));
            }
            object_schema(props)
        }
    }
}

// ---------------------------------------------------------------------------
// Schema tree construction
// ---------------------------------------------------------------------------

fn object_schema(properties: Map<String, Value>) -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            ("properties".to_string(), Value::Object(properties)),
            ("additionalProperties".to_string(), Value::Bool(false)),
        ]
        .into_iter()
        .collect(),
    )
}

fn unknown_object_schema() -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            (
                "additionalProperties".to_string(),
                Value::Object(Map::new()),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn insert_schema_at_path_segments(root: &mut Value, path_segments: &[String], leaf: Value) {
    if path_segments.is_empty() {
        return;
    }
    insert_schema_at_parts(root, path_segments, leaf);
}

fn apply_values_descriptions(root: &mut Value, descriptions: &BTreeMap<String, String>) {
    for (path, description) in descriptions {
        let path_segments: Vec<String> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
        apply_description_at_path_segments(root, &path_segments, description);
    }
}

fn apply_description_at_path_segments(
    node: &mut Value,
    path_segments: &[String],
    description: &str,
) {
    if path_segments.is_empty() {
        set_schema_description(node, description);
        return;
    }

    let Some((head, tail)) = path_segments.split_first() else {
        return;
    };

    let Value::Object(obj) = node else {
        return;
    };

    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(variants)) = obj.get_mut(key) {
            for variant in variants {
                apply_description_at_path_segments(variant, path_segments, description);
            }
        }
    }

    if head == "*" {
        if let Some(items) = obj.get_mut("items") {
            apply_description_at_path_segments(items, tail, description);
        }
        return;
    }

    if head == MAP_WILDCARD_SEGMENT {
        if let Some(additional_properties) = obj.get_mut("additionalProperties") {
            apply_description_at_path_segments(additional_properties, tail, description);
        }
        return;
    }

    let Some(properties) = obj.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(child) = properties.get_mut(head) else {
        return;
    };
    apply_description_at_path_segments(child, tail, description);
}

fn set_schema_description(node: &mut Value, description: &str) {
    if description.trim().is_empty() {
        return;
    }

    if let Value::Object(obj) = node {
        obj.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
}

fn ensure_object_schema(v: &mut Value) {
    match v {
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str) != Some("object") {
                obj.insert("type".to_string(), Value::String("object".to_string()));
            }
            obj.entry("properties".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if obj.get("properties").and_then(|p| p.as_object()).is_none() {
                obj.insert("properties".to_string(), Value::Object(Map::new()));
            }
            obj.entry("additionalProperties".to_string())
                .or_insert(Value::Bool(false));

            let has_structure = obj
                .get("properties")
                .and_then(|v| v.as_object())
                .is_some_and(|m| !m.is_empty())
                || obj
                    .get("patternProperties")
                    .and_then(|v| v.as_object())
                    .is_some_and(|m| !m.is_empty())
                || obj
                    .get("required")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| !a.is_empty());

            let ap_is_empty_schema = obj
                .get("additionalProperties")
                .and_then(|v| v.as_object())
                .is_some_and(serde_json::Map::is_empty);

            if has_structure && ap_is_empty_schema {
                obj.insert("additionalProperties".to_string(), Value::Bool(false));
            }
        }
        _ => {
            *v = object_schema(Map::new());
        }
    }
}

fn ensure_array_schema(v: &mut Value) {
    match v {
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str) != Some("array") {
                obj.insert("type".to_string(), Value::String("array".to_string()));
            }
            obj.entry("items".to_string()).or_insert(Value::Null);
        }
        _ => {
            *v = Value::Object(
                [
                    ("type".to_string(), Value::String("array".to_string())),
                    ("items".to_string(), Value::Null),
                ]
                .into_iter()
                .collect(),
            );
        }
    }
}

fn ensure_items_schema(array_schema: &mut Value) -> &mut Value {
    array_schema
        .as_object_mut()
        .and_then(|o| o.get_mut("items"))
        .expect("array schema must have items")
}

fn clear_exact_empty_constraint_for_descendant(node: &mut Value) {
    if let Value::Object(obj) = node
        && obj.get("maxProperties").and_then(Value::as_u64) == Some(0)
    {
        obj.remove("maxProperties");
    }
}

const MAP_WILDCARD_SEGMENT: &str = "__any__";

fn is_object_like_schema(v: &Value) -> bool {
    match schema_type(v) {
        Some("object") => true,
        Some(_) => false,
        None => v.as_object().is_some_and(|o| {
            o.contains_key("properties")
                || o.contains_key("additionalProperties")
                || o.contains_key("patternProperties")
                || o.contains_key("required")
        }),
    }
}

fn is_array_like_schema(v: &Value) -> bool {
    match schema_type(v) {
        Some("array") => true,
        Some(_) => false,
        None => v.as_object().is_some_and(|o| o.contains_key("items")),
    }
}

fn union_key(obj: &Map<String, Value>) -> Option<&'static str> {
    if obj.get("anyOf").and_then(Value::as_array).is_some() {
        Some("anyOf")
    } else if obj.get("oneOf").and_then(Value::as_array).is_some() {
        Some("oneOf")
    } else {
        None
    }
}

fn take_union_variants(obj: &mut Map<String, Value>, key: &str) -> Option<Vec<Value>> {
    let Value::Array(variants) = obj.remove(key).unwrap_or_else(|| Value::Array(Vec::new())) else {
        return None;
    };
    Some(variants)
}

fn push_union_structural_constraints_down(obj: &mut Map<String, Value>, variants: &mut [Value]) {
    let structural_keys = [
        "type",
        "properties",
        "additionalProperties",
        "patternProperties",
        "required",
        "items",
    ];
    let mut structural = Map::new();
    for k in structural_keys {
        if let Some(v) = obj.remove(k) {
            structural.insert(k.to_string(), v);
        }
    }

    if structural.is_empty() {
        return;
    }

    let structural_schema = Value::Object(structural);
    for v in variants {
        let compatible = if is_array_like_schema(&structural_schema) {
            is_array_like_schema(v)
        } else {
            is_object_like_schema(v)
        };

        if compatible {
            let existing = std::mem::replace(v, Value::Null);
            *v = merge_two_schemas(existing, structural_schema.clone());
        }
    }
}

fn insert_schema_into_union_variants(
    variants: &mut [Value],
    path_segments: &[String],
    leaf: &Value,
) -> bool {
    let head = path_segments[0].as_str();
    let mut touched = false;
    for v in variants {
        let compatible = if head == "*" {
            is_array_like_schema(v)
        } else {
            is_object_like_schema(v)
        };

        if compatible {
            insert_schema_at_parts(v, path_segments, leaf.clone());
            touched = true;
        }
    }
    touched
}

fn new_union_variant_for_head(head: &str) -> Value {
    if head == "*" {
        Value::Object(
            [
                ("type".to_string(), Value::String("array".to_string())),
                ("items".to_string(), Value::Null),
            ]
            .into_iter()
            .collect(),
        )
    } else {
        object_schema(Map::new())
    }
}

fn insert_schema_at_union(
    obj: &mut Map<String, Value>,
    key: &'static str,
    path_segments: &[String],
    leaf: Value,
) {
    let Some(mut variants) = take_union_variants(obj, key) else {
        return;
    };

    push_union_structural_constraints_down(obj, &mut variants);

    let touched = insert_schema_into_union_variants(&mut variants, path_segments, &leaf);
    if !touched {
        let mut new_variant = new_union_variant_for_head(path_segments[0].as_str());
        insert_schema_at_parts(&mut new_variant, path_segments, leaf);
        variants.push(new_variant);
    }

    obj.insert(key.to_string(), Value::Array(variants));
}

fn insert_schema_at_parts(node: &mut Value, path_segments: &[String], leaf: Value) {
    if path_segments.is_empty() {
        return;
    }

    // Union-aware insertion: when a value path is a union (e.g. `anyOf: [object, string]`),
    // inserting nested schemas must update the appropriate variant(s) rather than forcing the
    // union node itself into an object/array schema.
    if let Value::Object(obj) = node
        && let Some(key) = union_key(obj)
    {
        insert_schema_at_union(obj, key, path_segments, leaf);
        return;
    }

    if path_segments[0] == MAP_WILDCARD_SEGMENT {
        if path_segments.len() > 1 {
            clear_exact_empty_constraint_for_descendant(node);
        }
        ensure_object_schema(node);
        let obj = node.as_object_mut().expect("object schema");
        let ap = obj
            .entry("additionalProperties")
            .or_insert_with(|| Value::Object(Map::new()));
        if ap.as_bool() == Some(false) {
            *ap = Value::Object(Map::new());
        }
        if path_segments.len() == 1 {
            let existing = std::mem::replace(ap, Value::Null);
            *ap = match existing {
                Value::Null => leaf,
                other => merge_two_schemas(other, leaf),
            };
        } else {
            clear_exact_empty_constraint_for_descendant(ap);
            insert_schema_at_parts(ap, &path_segments[1..], leaf);
        }
        return;
    }

    if path_segments[0] == "*" {
        if !is_empty_schema(node) && !is_array_like_schema(node) {
            let existing = std::mem::replace(node, Value::Null);
            let mut array_variant = new_union_variant_for_head("*");
            insert_schema_at_parts(&mut array_variant, path_segments, leaf);
            *node = union_schema_list(vec![existing, array_variant]);
            return;
        }
        ensure_array_schema(node);
        let items = ensure_items_schema(node);
        if path_segments.len() == 1 {
            let existing = std::mem::replace(items, Value::Null);
            *items = match existing {
                Value::Null => leaf,
                other => merge_two_schemas(other, leaf),
            };
        } else {
            insert_schema_at_parts(items, &path_segments[1..], leaf);
        }
        return;
    }

    if path_segments.len() > 1 {
        clear_exact_empty_constraint_for_descendant(node);
    }
    ensure_object_schema(node);
    let props = node
        .as_object_mut()
        .and_then(|o| o.get_mut("properties"))
        .and_then(|v| v.as_object_mut())
        .expect("object schema must have properties");

    if path_segments.len() == 1 {
        let key = path_segments[0].clone();
        match props.entry(key) {
            serde_json::map::Entry::Vacant(entry) => {
                entry.insert(leaf);
            }
            serde_json::map::Entry::Occupied(mut entry) => {
                let existing = std::mem::replace(entry.get_mut(), Value::Null);
                *entry.get_mut() = merge_two_schemas(existing, leaf);
            }
        }
        return;
    }

    let key = path_segments[0].clone();
    let child = props.entry(key).or_insert_with(|| {
        if path_segments.get(1).is_some_and(|segment| segment == "*") {
            new_union_variant_for_head("*")
        } else {
            object_schema(Map::new())
        }
    });
    if path_segments.get(1).is_none_or(|segment| segment != "*") {
        clear_exact_empty_constraint_for_descendant(child);
    }
    insert_schema_at_parts(child, &path_segments[1..], leaf);
}

#[cfg(test)]
mod tests;
