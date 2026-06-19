use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use helm_schema_core::{ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, ValueKind, YamlPath};
use helm_schema_ir::{ContractPathSchemaEvidence, MetadataFieldKind};
use serde_json::{Map, Value};

use crate::merge::merge_schema_list;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::ResolvePolicy;
use crate::schema_model::type_schema;

pub(crate) struct ContractEvidenceIndex {
    referenced_value_paths: BTreeSet<String>,
    provider_schemas_by_value_path: BTreeMap<String, Vec<Arc<ProviderSchemaCandidate>>>,
    metadata_schema_by_value_path: BTreeMap<String, Value>,
    guard_constraint_schema_by_value_path: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProviderSchemaLookupKey {
    resource: ResourceRef,
    path: YamlPath,
    kind: ValueKind,
    is_self_range_collection: bool,
}

impl ContractEvidenceIndex {
    #[tracing::instrument(skip_all, fields(provider_uses))]
    pub(crate) fn from_schema_evidence(
        schema_evidence_by_path: &BTreeMap<String, ContractPathSchemaEvidence>,
        provider: &dyn ResourceSchemaOracle,
    ) -> Self {
        let resolve_policy = ResolvePolicy::default();
        let referenced_value_paths = schema_evidence_by_path
            .iter()
            .filter(|(_, evidence)| evidence.is_referenced_value_path)
            .map(|(path, _)| path.clone())
            .collect();
        let metadata_schema_by_value_path = schema_evidence_by_path
            .iter()
            .filter_map(|(path, evidence)| {
                if evidence.metadata_field_kinds.is_empty() {
                    return None;
                }
                let schemas = evidence
                    .metadata_field_kinds
                    .iter()
                    .copied()
                    .map(metadata_field_schema)
                    .collect::<Vec<_>>();
                Some((path.clone(), merge_schema_list(schemas)))
            })
            .collect();
        let guard_constraint_schema_by_value_path = schema_evidence_by_path
            .iter()
            .filter_map(|(path, evidence)| {
                let schemas = evidence
                    .guard_constraints
                    .iter()
                    .filter_map(|constraint| resolve_policy.guard_constraint_schema(constraint))
                    .collect::<Vec<_>>();
                (!schemas.is_empty()).then(|| (path.clone(), merge_schema_list(schemas)))
            })
            .collect();
        let provider_schema_uses = schema_evidence_by_path
            .values()
            .flat_map(|evidence| evidence.provider_schema_uses.iter());
        let provider_use_count = schema_evidence_by_path
            .values()
            .map(|evidence| evidence.provider_schema_uses.len())
            .sum::<usize>();
        tracing::Span::current().record("provider_uses", provider_use_count);
        let mut provider_schema_cache: HashMap<
            ProviderSchemaLookupKey,
            Option<Arc<ProviderSchemaCandidate>>,
        > = HashMap::new();
        let mut provider_schemas_by_value_path: BTreeMap<
            String,
            Vec<Arc<ProviderSchemaCandidate>>,
        > = BTreeMap::new();

        for provider_use in provider_schema_uses {
            let lookup_key = ProviderSchemaLookupKey {
                resource: provider_use.resource.clone(),
                path: provider_use.path.clone(),
                kind: provider_use.kind,
                is_self_range_collection: provider_use.is_self_range_collection,
            };
            let schema = match provider_schema_cache.entry(lookup_key) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.get().clone(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let schema = lookup_provider_schema(provider, provider_use, &resolve_policy);
                    entry.insert(schema.clone());
                    schema
                }
            };
            if let Some(schema) = schema {
                let provider_schemas = provider_schemas_by_value_path
                    .entry(provider_use.value_path.clone())
                    .or_default();
                if !provider_schemas
                    .iter()
                    .any(|existing| Arc::ptr_eq(existing, &schema))
                {
                    provider_schemas.push(schema);
                }
            }
        }

        Self {
            referenced_value_paths,
            provider_schemas_by_value_path,
            metadata_schema_by_value_path,
            guard_constraint_schema_by_value_path,
        }
    }

    pub(crate) fn referenced_value_paths(&self) -> &BTreeSet<String> {
        &self.referenced_value_paths
    }

    pub(crate) fn take_referenced_value_paths(&mut self) -> BTreeSet<String> {
        std::mem::take(&mut self.referenced_value_paths)
    }

    pub(crate) fn take_provider_schemas(
        &mut self,
        value_path: &str,
    ) -> Vec<Arc<ProviderSchemaCandidate>> {
        self.provider_schemas_by_value_path
            .remove(value_path)
            .unwrap_or_default()
    }

    pub(crate) fn take_metadata_schema(&mut self, value_path: &str) -> Value {
        self.metadata_schema_by_value_path
            .remove(value_path)
            .unwrap_or_else(crate::schema_model::empty_schema)
    }

    pub(crate) fn take_guard_constraint_schema(&mut self, value_path: &str) -> Value {
        self.guard_constraint_schema_by_value_path
            .remove(value_path)
            .unwrap_or_else(crate::schema_model::empty_schema)
    }
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

fn string_map_schema() -> Value {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("additionalProperties".to_string(), type_schema("string"));
    Value::Object(schema)
}
