use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use helm_schema_core::{ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, ValueKind, YamlPath};
use helm_schema_ir::{ContractPathSchemaEvidence, ContractSchemaSignals, MetadataFieldKind};
use serde_json::{Map, Value};

use crate::merge::merge_schema_list;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::ResolvePolicy;
use crate::schema_model::type_schema;

pub(crate) struct ContractEvidenceIndex {
    referenced_value_paths: BTreeSet<String>,
    evidence_by_value_path: BTreeMap<String, IndexedContractPathEvidence>,
}

pub(crate) struct IndexedContractPathEvidence {
    pub(crate) contract: ContractPathSchemaEvidence,
    pub(crate) provider_schemas: Vec<Arc<ProviderSchemaCandidate>>,
    pub(crate) type_hint_schema: Value,
    pub(crate) metadata_schema: Value,
    pub(crate) guard_predicate_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProviderSchemaLookupKey {
    resource: ResourceRef,
    path: YamlPath,
    kind: ValueKind,
    is_self_range_collection: bool,
}

impl ContractEvidenceIndex {
    pub(crate) fn from_contract_signals(
        contract_signals: &ContractSchemaSignals,
        provider: &dyn ResourceSchemaOracle,
    ) -> Self {
        Self::from_schema_evidence(contract_signals.schema_evidence_by_value_path(), provider)
    }

    #[tracing::instrument(skip_all, fields(provider_uses))]
    fn from_schema_evidence(
        schema_evidence_by_path: &BTreeMap<String, ContractPathSchemaEvidence>,
        provider: &dyn ResourceSchemaOracle,
    ) -> Self {
        let resolve_policy = ResolvePolicy::default();
        let referenced_value_paths = schema_evidence_by_path
            .iter()
            .filter(|(_, evidence)| evidence.is_referenced_value_path)
            .map(|(path, _)| path.clone())
            .collect();
        let provider_use_count = schema_evidence_by_path
            .values()
            .map(|evidence| evidence.provider_schema_uses.len())
            .sum::<usize>();
        tracing::Span::current().record("provider_uses", provider_use_count);
        let mut provider_schema_cache: HashMap<
            ProviderSchemaLookupKey,
            Option<Arc<ProviderSchemaCandidate>>,
        > = HashMap::new();
        let evidence_by_value_path = schema_evidence_by_path
            .iter()
            .map(|(path, evidence)| {
                (
                    path.clone(),
                    index_contract_path_evidence(
                        evidence,
                        provider,
                        &resolve_policy,
                        &mut provider_schema_cache,
                    ),
                )
            })
            .collect();

        Self {
            referenced_value_paths,
            evidence_by_value_path,
        }
    }

    #[tracing::instrument(skip_all, fields(provider_uses = evidence.provider_schema_uses.len()))]
    pub(crate) fn from_path_evidence(
        evidence: &ContractPathSchemaEvidence,
        provider: &dyn ResourceSchemaOracle,
    ) -> Self {
        let resolve_policy = ResolvePolicy::default();
        let mut provider_schema_cache = HashMap::new();
        let referenced_value_paths = evidence
            .is_referenced_value_path
            .then(|| evidence.value_path.clone())
            .into_iter()
            .collect();
        let evidence_by_value_path = [(
            evidence.value_path.clone(),
            index_contract_path_evidence(
                evidence,
                provider,
                &resolve_policy,
                &mut provider_schema_cache,
            ),
        )]
        .into_iter()
        .collect();

        Self {
            referenced_value_paths,
            evidence_by_value_path,
        }
    }

    pub(crate) fn referenced_value_paths(&self) -> &BTreeSet<String> {
        &self.referenced_value_paths
    }

    pub(crate) fn take_referenced_value_paths(&mut self) -> BTreeSet<String> {
        std::mem::take(&mut self.referenced_value_paths)
    }

    pub(crate) fn take_path_evidence(
        &mut self,
        value_path: &str,
    ) -> Option<IndexedContractPathEvidence> {
        self.evidence_by_value_path.remove(value_path)
    }
}

fn index_contract_path_evidence(
    evidence: &ContractPathSchemaEvidence,
    provider: &dyn ResourceSchemaOracle,
    resolve_policy: &ResolvePolicy,
    provider_schema_cache: &mut HashMap<
        ProviderSchemaLookupKey,
        Option<Arc<ProviderSchemaCandidate>>,
    >,
) -> IndexedContractPathEvidence {
    let provider_schemas = provider_schemas_for_path_evidence(
        evidence,
        provider,
        resolve_policy,
        provider_schema_cache,
    );
    let type_hint_schema = if evidence.type_hints.is_empty() {
        crate::schema_model::empty_schema()
    } else {
        merge_type_hint_schemas(&evidence.type_hints)
    };
    let metadata_schema = if evidence.metadata_field_kinds.is_empty() {
        crate::schema_model::empty_schema()
    } else {
        merge_schema_list(
            evidence
                .metadata_field_kinds
                .iter()
                .copied()
                .map(metadata_field_schema)
                .collect(),
        )
    };
    let guard_predicate_schema = merge_schema_list(
        evidence
            .guard_predicates
            .iter()
            .filter_map(|predicate| {
                resolve_policy.guard_predicate_schema(&evidence.value_path, predicate)
            })
            .collect(),
    );

    IndexedContractPathEvidence {
        contract: evidence.clone(),
        provider_schemas,
        type_hint_schema,
        metadata_schema,
        guard_predicate_schema,
    }
}

fn provider_schemas_for_path_evidence(
    evidence: &ContractPathSchemaEvidence,
    provider: &dyn ResourceSchemaOracle,
    resolve_policy: &ResolvePolicy,
    provider_schema_cache: &mut HashMap<
        ProviderSchemaLookupKey,
        Option<Arc<ProviderSchemaCandidate>>,
    >,
) -> Vec<Arc<ProviderSchemaCandidate>> {
    let mut provider_schemas = Vec::new();

    for provider_use in &evidence.provider_schema_uses {
        let lookup_key = ProviderSchemaLookupKey {
            resource: provider_use.resource.clone(),
            path: provider_use.path.clone(),
            kind: provider_use.kind,
            is_self_range_collection: provider_use.is_self_range_collection,
        };
        let schema = match provider_schema_cache.entry(lookup_key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.get().clone(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let schema = lookup_provider_schema(provider, provider_use, resolve_policy);
                entry.insert(schema.clone());
                schema
            }
        };
        if let Some(schema) = schema
            && !provider_schemas
                .iter()
                .any(|existing| Arc::ptr_eq(existing, &schema))
        {
            provider_schemas.push(schema);
        }
    }

    provider_schemas
}

#[tracing::instrument(
    skip_all,
    fields(
        resource_kind = provider_use.resource.kind.as_str(),
        resource_api_version = provider_use.resource.api_version.as_str(),
        path_len = provider_use.path.0.len(),
    )
)]
fn lookup_provider_schema(
    provider: &dyn ResourceSchemaOracle,
    provider_use: &ProviderSchemaUse,
    resolve_policy: &ResolvePolicy,
) -> Option<Arc<ProviderSchemaCandidate>> {
    provider
        .schema_fragment_for_use(provider_use)
        .and_then(|fragment| {
            fragment.try_map_schema(|schema| {
                resolve_policy.provider_schema_for_value_use(schema, provider_use)
            })
        })
        .map(ProviderSchemaCandidate::from_provider_fragment)
        .map(Arc::new)
}

fn metadata_field_schema(field: MetadataFieldKind) -> Value {
    match field {
        MetadataFieldKind::StringMap => string_map_schema(),
        MetadataFieldKind::Name | MetadataFieldKind::Namespace => type_schema("string"),
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

fn string_map_schema() -> Value {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("additionalProperties".to_string(), type_schema("string"));
    Value::Object(schema)
}

#[cfg(test)]
mod tests {
    use helm_schema_core::{ProviderOrigin, ProviderSchemaFragment};
    use helm_schema_ir::{ContractValuePathFacts, ResourceRef};
    use test_util::prelude::sim_assert_eq;

    use super::*;

    #[derive(Debug)]
    struct StringProvider;

    impl ResourceSchemaOracle for StringProvider {
        fn schema_fragment_for_use(
            &self,
            _use_: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            Some(ProviderSchemaFragment::new(type_schema("string")))
        }

        fn schema_fragment_for_resource_path(
            &self,
            _resource: &ResourceRef,
            _path: &YamlPath,
        ) -> Option<ProviderSchemaFragment> {
            None
        }

        fn origin(&self) -> ProviderOrigin {
            ProviderOrigin::KubernetesOpenApi
        }

        fn has_resource(&self, _resource: &ResourceRef) -> bool {
            true
        }
    }

    #[test]
    fn indexed_contract_evidence_keeps_provider_schema_path_local() {
        let provider_use = ProviderSchemaUse {
            value_path: "other".to_string(),
            path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            kind: ValueKind::Scalar,
            resource: ResourceRef {
                api_version: "v1".to_string(),
                kind: "Service".to_string(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            },
            is_self_range_collection: false,
        };
        let mut schema_evidence_by_path = BTreeMap::new();
        schema_evidence_by_path.insert(
            "actual".to_string(),
            ContractPathSchemaEvidence {
                value_path: "actual".to_string(),
                is_referenced_value_path: true,
                provider_schema_uses: vec![provider_use],
                facts: ContractValuePathFacts {
                    has_render_use: true,
                    ..ContractValuePathFacts::default()
                },
                ..ContractPathSchemaEvidence::default()
            },
        );

        let mut index =
            ContractEvidenceIndex::from_schema_evidence(&schema_evidence_by_path, &StringProvider);

        let actual = index
            .take_path_evidence("actual")
            .expect("outer path evidence should be indexed");
        sim_assert_eq!(
            actual.provider_schemas.len(),
            1,
            "provider schema should stay attached to the outer contract evidence path"
        );
        assert!(
            index.take_path_evidence("other").is_none(),
            "inner provider-use value_path must not route evidence to a different path"
        );
    }
}
