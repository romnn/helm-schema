use std::collections::BTreeSet;
use std::sync::Arc;

use helm_schema_core::ResourceSchemaOracle;
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{ContractPathSchemaEvidence, ContractSchemaSignals};

use crate::contract_evidence_index::{ContractEvidenceIndex, IndexedContractPathEvidence};
use crate::merge::merge_schema_list;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::{ResolvePolicy, ValuePathSchemaFacts, ValuePathSchemaInputs};
use crate::schema_model::{empty_schema, is_empty_schema};
use crate::values_yaml::{
    ValuePathCaches, ValuesYamlPathFacts, ValuesYamlPathInfo, build_value_path_caches,
};

pub(crate) struct ResolvedPathSchema {
    pub(crate) value_path: String,
    pub(crate) path_segments: Vec<String>,
    pub(crate) schema: Value,
    pub(crate) values_yaml_schema: Value,
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

pub(crate) struct PathSchemaResolver {
    evidence_index: ContractEvidenceIndex,
    path_caches: ValuePathCaches,
    resolve_policy: ResolvePolicy,
}

impl PathSchemaResolver {
    pub(crate) fn new(
        contract_signals: &ContractSchemaSignals,
        values_yaml_doc: &YamlValue,
        provider: &dyn ResourceSchemaOracle,
    ) -> Self {
        let schema_evidence_by_path = contract_signals.schema_evidence_by_value_path();
        let evidence_index =
            ContractEvidenceIndex::from_contract_signals(contract_signals, provider);
        let pruned_parent_value_paths = schema_evidence_by_path
            .iter()
            .filter_map(|(path, evidence)| {
                (evidence.facts.has_referenced_descendants && !evidence.facts.used_as_fragment)
                    .then_some(path.clone())
            })
            .collect();
        let path_caches = build_value_path_caches(
            values_yaml_doc,
            evidence_index.referenced_value_paths(),
            &pruned_parent_value_paths,
        );
        Self {
            evidence_index,
            path_caches,
            resolve_policy: ResolvePolicy,
        }
    }

    pub(crate) fn resolve_single_path_evidence(
        evidence: &ContractPathSchemaEvidence,
        values_yaml_doc: &YamlValue,
        provider: &dyn ResourceSchemaOracle,
    ) -> Option<ResolvedPathSchema> {
        let evidence_index = ContractEvidenceIndex::from_path_evidence(evidence, provider);
        let mut resolver =
            Self::from_evidence_index(evidence_index, values_yaml_doc, BTreeSet::new());
        resolver.resolve_path(evidence.value_path.clone())
    }

    pub(crate) fn resolve_all(mut self) -> Vec<ResolvedPathSchema> {
        let referenced_value_paths = self.evidence_index.take_referenced_value_paths();
        referenced_value_paths
            .into_iter()
            .filter_map(|value_path| self.resolve_path(value_path))
            .collect()
    }

    fn resolve_path(&mut self, value_path: String) -> Option<ResolvedPathSchema> {
        let path_segments = self.path_caches.path_segments.get(&value_path)?.clone();
        let evidence = self.evidence_index.take_path_evidence(&value_path)?;
        let values_yaml_info = self.path_caches.values_yaml.get(&value_path);
        let PathSchemaEvidence {
            policy_inputs,
            provider_schema_candidate,
        } = Self::path_schema_evidence(evidence, values_yaml_info);
        let merged = self
            .resolve_policy
            .resolve_schema_for_value_path(policy_inputs);
        let provider_schema_candidate = provider_schema_candidate
            .filter(|provider_schema| provider_schema.survives_as(&merged));

        Some(ResolvedPathSchema {
            value_path,
            path_segments,
            schema: merged,
            values_yaml_schema: values_yaml_info
                .map(|path_info| path_info.schema.clone())
                .unwrap_or_else(empty_schema),
            provider_schema_candidate,
        })
    }

    fn from_evidence_index(
        evidence_index: ContractEvidenceIndex,
        values_yaml_doc: &YamlValue,
        pruned_parent_value_paths: BTreeSet<String>,
    ) -> Self {
        let path_caches = build_value_path_caches(
            values_yaml_doc,
            evidence_index.referenced_value_paths(),
            &pruned_parent_value_paths,
        );
        Self {
            evidence_index,
            path_caches,
            resolve_policy: ResolvePolicy,
        }
    }

    fn path_schema_evidence(
        evidence: IndexedContractPathEvidence,
        values_yaml_info: Option<&ValuesYamlPathInfo>,
    ) -> PathSchemaEvidence {
        let provider_schema =
            Self::provider_schema_for_path(evidence.provider_schemas, evidence.metadata_schema);
        let values_yaml_facts = values_yaml_info
            .map_or_else(ValuesYamlPathFacts::absent, |path_info| path_info.facts());
        let facts = ValuePathSchemaFacts::new(evidence.contract.facts, values_yaml_facts);

        let values_yaml_schema = values_yaml_info
            .map(|path_info| path_info.schema.clone())
            .unwrap_or_else(empty_schema);
        PathSchemaEvidence {
            policy_inputs: ValuePathSchemaInputs {
                facts,
                provider_schema: provider_schema.schema,
                values_yaml_schema,
                guard_predicate_schema: evidence.guard_predicate_schema,
                type_hint_schema: evidence.type_hint_schema,
            },
            provider_schema_candidate: provider_schema.provider_schema_candidate,
        }
    }

    fn provider_schema_for_path(
        provider_schemas: Vec<Arc<ProviderSchemaCandidate>>,
        metadata_schema: Value,
    ) -> ProviderSchemaForPath {
        let single_provider_schema = match provider_schemas.as_slice() {
            [schema] => Some(schema.clone()),
            _ => None,
        };
        let provider_schema = if let Some(provider_schema) = single_provider_schema.as_deref() {
            provider_schema.schema().clone()
        } else {
            merge_schema_list(
                provider_schemas
                    .into_iter()
                    .map(|schema| schema.schema().clone())
                    .collect(),
            )
        };
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
