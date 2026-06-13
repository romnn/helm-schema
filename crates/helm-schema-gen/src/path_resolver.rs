use std::collections::BTreeMap;

use serde_json::Value;
use serde_yaml::Value as YamlValue;

use helm_schema_ir::ChartFacts;
use helm_schema_k8s::type_schema;

use crate::merge::merge_schema_list;
use crate::path_metadata::PathMetadata;
use crate::path_schema::{
    generalize_fixed_object_schema_to_open_map, merge_explicit_empty_placeholder,
    open_fragment_values_schema, preserve_explicit_empty_placeholder,
    values_yaml_schema_for_path as base_values_yaml_schema_for_path,
};
use crate::resolve_policy::{ResolvePolicy, ValuePathSchemaInputs};
use crate::schema_model::{
    add_null_schema, empty_schema, is_empty_schema, is_scalar_like_schema, is_scalar_schema,
    is_string_like_schema,
};
use crate::use_signals::UseSignals;
use crate::values_yaml::{ValuePathCaches, ValuesYamlPathInfo, build_value_path_caches};

pub(crate) struct ResolvedPathSchema {
    pub(crate) path_segments: Vec<String>,
    pub(crate) schema: Value,
}

pub(crate) struct PathSchemaResolver<'a> {
    signals: UseSignals,
    path_metadata: &'a PathMetadata,
    path_caches: ValuePathCaches,
    type_hints: &'a BTreeMap<String, Vec<Value>>,
    chart_facts: &'a ChartFacts,
    resolve_policy: ResolvePolicy,
}

impl<'a> PathSchemaResolver<'a> {
    pub(crate) fn new(
        signals: UseSignals,
        path_metadata: &'a PathMetadata,
        values_yaml_doc: &YamlValue,
        type_hints: &'a BTreeMap<String, Vec<Value>>,
        chart_facts: &'a ChartFacts,
    ) -> Self {
        let path_caches = build_value_path_caches(values_yaml_doc, &signals.referenced_value_paths);
        Self {
            signals,
            path_metadata,
            path_caches,
            type_hints,
            chart_facts,
            resolve_policy: ResolvePolicy::default(),
        }
    }

    pub(crate) fn resolve_all(mut self) -> Vec<ResolvedPathSchema> {
        let referenced_value_paths = std::mem::take(&mut self.signals.referenced_value_paths);
        referenced_value_paths
            .into_iter()
            .filter_map(|value_path| self.resolve_path(value_path))
            .collect()
    }

    fn resolve_path(&mut self, value_path: String) -> Option<ResolvedPathSchema> {
        let path_segments = self.path_caches.path_segments.get(&value_path)?.clone();
        let path_fact = self
            .chart_facts
            .path_facts
            .get(&value_path)
            .cloned()
            .unwrap_or_default();
        let used_as_fragment = self
            .signals
            .value_paths_used_as_fragment
            .contains(&value_path);
        let is_ranged_source = self.signals.ranged_value_paths.contains(&value_path);

        let provider_schema = self.provider_schema_for_path(&value_path);
        let type_hint_schema = self
            .type_hints
            .get(&value_path)
            .cloned()
            .map_or_else(empty_schema, merge_schema_list);
        let guard_constraint_schema = self
            .signals
            .guard_constraints_by_value_path
            .remove(&value_path)
            .map_or_else(empty_schema, merge_schema_list);
        let partial_scalar_schema = self.partial_scalar_schema_for_path(
            &value_path,
            &provider_schema,
            &type_hint_schema,
            &guard_constraint_schema,
        );
        let values_yaml_info = self.path_caches.values_yaml.get(&value_path);

        let has_explicit_null_scalar_default = values_yaml_info
            .is_some_and(|path_info| path_info.is_explicit_null)
            && (is_scalar_like_schema(&type_hint_schema)
                || is_scalar_like_schema(&guard_constraint_schema));
        let path_is_nullable = self.path_metadata.nullable_paths.contains(&value_path)
            || self.type_hints.contains_key(&value_path)
            || has_explicit_null_scalar_default;
        let preserve_explicit_null_default = path_is_nullable
            && values_yaml_info.is_some_and(|path_info| path_info.is_explicit_null);
        let preserve_empty_string_fallback = values_yaml_info
            .is_some_and(|path_info| path_info.is_empty_string)
            && ((path_fact.has_render_use && path_fact.all_render_uses_self_guarded)
                || is_scalar_like_schema(&type_hint_schema)
                || is_scalar_like_schema(&guard_constraint_schema));

        let values_yaml_schema = self.adjusted_values_yaml_schema_for_path(
            values_yaml_info,
            &path_fact,
            &provider_schema,
            used_as_fragment,
            is_ranged_source,
            &value_path,
        );
        let provider_schema = if used_as_fragment
            && is_scalar_schema(&values_yaml_schema)
            && (is_scalar_like_schema(&type_hint_schema)
                || is_scalar_like_schema(&guard_constraint_schema))
        {
            self.resolve_policy
                .restrict_to_scalar_domain(provider_schema.clone())
                .unwrap_or(provider_schema)
        } else {
            provider_schema
        };

        let merged = self
            .resolve_policy
            .resolve_schema_for_value_path(ValuePathSchemaInputs {
                has_referenced_descendants: self
                    .path_metadata
                    .paths_with_descendants
                    .contains(&value_path),
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
        let schema = if (preserve_explicit_null_default
            || (is_scalar_like_schema(&merged)
                && self.path_metadata.nullable_paths.contains(&value_path)))
            && !is_empty_schema(&merged)
        {
            add_null_schema(merged)
        } else if preserve_explicit_null_default {
            type_schema("null")
        } else if should_preserve_empty_placeholder {
            if let Some(path_info) = values_yaml_info {
                merge_explicit_empty_placeholder(merged, path_info)
            } else {
                merged
            }
        } else {
            merged
        };

        Some(ResolvedPathSchema {
            path_segments,
            schema,
        })
    }

    fn provider_schema_for_path(&mut self, value_path: &str) -> Value {
        let provider_schemas = self
            .signals
            .provider_schemas_by_value_path
            .remove(value_path)
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
        let metadata_schema = self
            .signals
            .metadata_schemas_by_value_path
            .remove(value_path)
            .map_or_else(empty_schema, merge_schema_list);

        merge_schema_list(vec![provider_schema, metadata_schema])
    }

    fn partial_scalar_schema_for_path(
        &self,
        value_path: &str,
        provider_schema: &Value,
        type_hint_schema: &Value,
        guard_constraint_schema: &Value,
    ) -> Value {
        let values_yaml_info = self.path_caches.values_yaml.get(value_path);
        if self.signals.partial_scalar_value_paths.contains(value_path)
            && is_empty_schema(provider_schema)
            && is_empty_schema(type_hint_schema)
            && is_empty_schema(guard_constraint_schema)
            && values_yaml_info.is_none_or(|path_info| is_empty_schema(&path_info.schema))
        {
            type_schema("string")
        } else {
            empty_schema()
        }
    }

    fn adjusted_values_yaml_schema_for_path(
        &self,
        values_yaml_info: Option<&ValuesYamlPathInfo>,
        path_fact: &helm_schema_ir::PathFact,
        provider_schema: &Value,
        used_as_fragment: bool,
        is_ranged_source: bool,
        value_path: &str,
    ) -> Value {
        let values_yaml_schema = values_yaml_info
            .map(|path_info| {
                base_values_yaml_schema_for_path(
                    path_info,
                    path_fact,
                    provider_schema,
                    used_as_fragment,
                    is_ranged_source,
                )
            })
            .unwrap_or_else(empty_schema);
        let values_yaml_schema = if used_as_fragment && is_empty_schema(provider_schema) {
            open_fragment_values_schema(values_yaml_schema)
        } else {
            values_yaml_schema
        };

        if self.signals.ranged_value_paths.contains(value_path)
            && values_yaml_info.is_some_and(|path_info| path_info.is_mapping)
        {
            generalize_fixed_object_schema_to_open_map(values_yaml_schema)
        } else {
            values_yaml_schema
        }
    }
}
