use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use serde_yaml::Value as YamlValue;

use helm_schema_ir::ContractPathSchemaEvidence;

use crate::merge::merge_schema_list;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::{ResolvePolicy, ValuePathSchemaFacts, ValuePathSchemaInputs};
use crate::schema_model::{empty_schema, is_empty_schema, is_string_like_schema, type_schema};
use crate::use_signals::UseSignals;
use crate::values_yaml::{ValuePathCaches, build_value_path_caches};

pub(crate) struct ResolvedPathSchema {
    pub(crate) value_path: String,
    pub(crate) path_segments: Vec<String>,
    pub(crate) schema: Value,
    pub(crate) provider_schema_candidate: Option<ProviderSchemaCandidate>,
}

struct ProviderSchemaForPath {
    schema: Value,
    provider_schema_candidate: Option<ProviderSchemaCandidate>,
}

struct PathSchemaEvidence {
    policy_inputs: ValuePathSchemaInputs,
    provider_schema_candidate: Option<ProviderSchemaCandidate>,
}

pub(crate) struct PathSchemaResolver<'a> {
    signals: UseSignals,
    schema_evidence_by_path: &'a BTreeMap<String, ContractPathSchemaEvidence>,
    path_caches: ValuePathCaches,
    resolve_policy: ResolvePolicy,
}

impl<'a> PathSchemaResolver<'a> {
    pub(crate) fn new(
        signals: UseSignals,
        schema_evidence_by_path: &'a BTreeMap<String, ContractPathSchemaEvidence>,
        values_yaml_doc: &YamlValue,
    ) -> Self {
        let pruned_parent_value_paths = schema_evidence_by_path
            .iter()
            .filter_map(|(path, evidence)| {
                (evidence.facts.has_referenced_descendants && !evidence.facts.used_as_fragment)
                    .then_some(path.clone())
            })
            .collect();
        let path_caches = build_value_path_caches(
            values_yaml_doc,
            &signals.referenced_value_paths,
            &pruned_parent_value_paths,
        );
        Self {
            signals,
            schema_evidence_by_path,
            path_caches,
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
        let PathSchemaEvidence {
            policy_inputs,
            provider_schema_candidate,
        } = self.path_schema_evidence(&value_path);
        let merged = self
            .resolve_policy
            .resolve_schema_for_value_path(policy_inputs);
        let provider_schema_candidate = provider_schema_candidate
            .filter(|provider_schema| provider_schema.survives_as(&merged));

        Some(ResolvedPathSchema {
            value_path,
            path_segments,
            schema: merged,
            provider_schema_candidate,
        })
    }

    fn path_schema_evidence(&mut self, value_path: &str) -> PathSchemaEvidence {
        let contract_evidence = self.schema_evidence_by_path.get(value_path);
        let contract_facts = contract_evidence
            .map(|evidence| evidence.facts)
            .unwrap_or_default();
        let provider_schema = self.provider_schema_for_path(value_path);
        let values_yaml_info = self.path_caches.values_yaml.get(value_path);
        let type_hint_schema = contract_evidence.map_or_else(empty_schema, |evidence| {
            merge_type_hint_schemas(&evidence.type_hints)
        });
        let guard_constraint_schema = self
            .signals
            .guard_constraints_by_value_path
            .remove(value_path)
            .map_or_else(empty_schema, merge_schema_list);

        let facts = ValuePathSchemaFacts {
            has_referenced_descendants: contract_facts.has_referenced_descendants,
            used_as_fragment: contract_facts.used_as_fragment,
            is_ranged_source: contract_facts.is_ranged_source,
            is_partial_scalar_value_path: contract_facts.is_partial_scalar_value_path,
            path_has_render_use: contract_facts.has_render_use,
            path_all_render_uses_self_guarded: contract_facts.all_render_uses_self_guarded,
            path_has_self_range_guard_render_use: contract_facts.has_self_range_guard_render_use,
            contract_path_is_nullable: contract_facts.is_nullable,
            values_yaml_has_no_schema_evidence: values_yaml_info
                .is_none_or(|path_info| is_empty_schema(&path_info.schema)),
            values_yaml_is_explicit_null: values_yaml_info
                .is_some_and(|path_info| path_info.is_explicit_null),
            values_yaml_is_empty_string: values_yaml_info
                .is_some_and(|path_info| path_info.is_empty_string),
            values_yaml_is_empty_map: values_yaml_info
                .is_some_and(|path_info| path_info.is_empty_map),
            values_yaml_is_mapping: values_yaml_info.is_some_and(|path_info| path_info.is_mapping),
        };

        let values_yaml_schema = values_yaml_info
            .map(|path_info| path_info.schema.clone())
            .unwrap_or_else(empty_schema);
        PathSchemaEvidence {
            policy_inputs: ValuePathSchemaInputs {
                facts,
                provider_schema: provider_schema.schema,
                values_yaml_schema,
                guard_constraint_schema,
                type_hint_schema,
            },
            provider_schema_candidate: provider_schema.provider_schema_candidate,
        }
    }

    fn provider_schema_for_path(&mut self, value_path: &str) -> ProviderSchemaForPath {
        let provider_schemas = self
            .signals
            .provider_schemas_by_value_path
            .remove(value_path)
            .unwrap_or_default();
        let single_provider_schema = match provider_schemas.as_slice() {
            [schema] => Some(schema.clone()),
            _ => None,
        };
        let provider_schema = if provider_schemas.len() > 1
            && provider_schemas
                .iter()
                .all(|schema| is_string_like_schema(schema.schema()))
        {
            type_schema("string")
        } else if let Some(provider_schema) = single_provider_schema.as_deref() {
            provider_schema.schema().clone()
        } else {
            merge_schema_list(
                provider_schemas
                    .into_iter()
                    .map(|schema| schema.schema().clone())
                    .collect(),
            )
        };
        let metadata_schema = self
            .signals
            .metadata_schemas_by_value_path
            .remove(value_path)
            .map_or_else(empty_schema, merge_schema_list);
        let provider_schema_candidate = if is_empty_schema(&metadata_schema) {
            single_provider_schema.as_deref().cloned()
        } else {
            None
        };

        ProviderSchemaForPath {
            schema: merge_schema_list(vec![provider_schema, metadata_schema]),
            provider_schema_candidate,
        }
    }
}

fn merge_type_hint_schemas(schema_types: &BTreeSet<String>) -> Value {
    merge_schema_list(
        schema_types
            .iter()
            .map(|schema_type| type_schema(schema_type))
            .collect(),
    )
}
