use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use helm_schema_core::{ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, ValueKind, YamlPath};
use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{ContractPathSchemaEvidence, ContractSchemaSignals, MetadataFieldKind};

use crate::merge::merge_schema_list;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::{ResolvePolicy, ValuePathSchemaFacts, ValuePathSchemaInputs};
use crate::schema_model::{empty_schema, is_empty_schema, type_schema};
use crate::values_yaml::{ValuesYamlPathFacts, ValuesYamlPathInfo, build_values_yaml_path_info};

pub(crate) struct ResolvedPathSchema {
    pub(crate) value_path: String,
    pub(crate) path_segments: Vec<String>,
    pub(crate) schema: Value,
    pub(crate) values_yaml_schema: Value,
    pub(crate) provider_schema_candidate: Option<ProviderSchemaCandidate>,
    pub(crate) used_as_pathless_fragment: bool,
    pub(crate) accepted_dependency_values_root_fragment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProviderSchemaLookupKey {
    resource: ResourceRef,
    path: YamlPath,
    kind: ValueKind,
    is_self_range_collection: bool,
}

pub(crate) struct PathSchemaResolver<'a> {
    schema_evidence_by_value_path: &'a BTreeMap<String, ContractPathSchemaEvidence>,
    referenced_value_paths: &'a BTreeSet<String>,
    values_yaml_info: BTreeMap<String, ValuesYamlPathInfo>,
    resolve_policy: ResolvePolicy,
    provider: &'a dyn ResourceSchemaOracle,
    provider_schema_cache: HashMap<ProviderSchemaLookupKey, Option<Arc<ProviderSchemaCandidate>>>,
}

impl<'a> PathSchemaResolver<'a> {
    pub(crate) fn new(
        contract_signals: &'a ContractSchemaSignals,
        values_yaml_doc: &YamlValue,
        provider: &'a dyn ResourceSchemaOracle,
    ) -> Self {
        let values_yaml_info = build_values_yaml_path_info(
            values_yaml_doc,
            contract_signals.referenced_value_paths(),
            contract_signals.pruned_parent_value_paths(),
        );
        Self {
            schema_evidence_by_value_path: contract_signals.schema_evidence_by_value_path(),
            referenced_value_paths: contract_signals.referenced_value_paths(),
            values_yaml_info,
            resolve_policy: ResolvePolicy,
            provider,
            provider_schema_cache: HashMap::new(),
        }
    }

    /// Resolve overlay/branch evidence in isolation, without a values.yaml
    /// document or a shared cache.
    pub(crate) fn resolve_single_path_evidence(
        evidence: &ContractPathSchemaEvidence,
        provider: &dyn ResourceSchemaOracle,
    ) -> ResolvedPathSchema {
        let path_segments = crate::split_value_path(&evidence.value_path);
        resolve_path_evidence(
            evidence.clone(),
            path_segments,
            None,
            provider,
            &ResolvePolicy,
            &mut HashMap::new(),
        )
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn resolve_all(mut self) -> Vec<ResolvedPathSchema> {
        let referenced_value_paths = self.referenced_value_paths;
        referenced_value_paths
            .iter()
            .filter_map(|value_path| self.resolve_path(value_path))
            .collect()
    }

    fn resolve_path(&mut self, value_path: &str) -> Option<ResolvedPathSchema> {
        let evidence = self
            .schema_evidence_by_value_path
            .get(value_path)
            .cloned()?;
        Some(resolve_path_evidence(
            evidence,
            crate::split_value_path(value_path),
            self.values_yaml_info.get(value_path),
            self.provider,
            &self.resolve_policy,
            &mut self.provider_schema_cache,
        ))
    }
}

fn resolve_path_evidence(
    evidence: ContractPathSchemaEvidence,
    path_segments: Vec<String>,
    values_yaml_info: Option<&ValuesYamlPathInfo>,
    provider: &dyn ResourceSchemaOracle,
    resolve_policy: &ResolvePolicy,
    provider_schema_cache: &mut HashMap<
        ProviderSchemaLookupKey,
        Option<Arc<ProviderSchemaCandidate>>,
    >,
) -> ResolvedPathSchema {
    let value_path = evidence.value_path.clone();
    let used_as_pathless_fragment = evidence.facts.used_as_pathless_fragment;
    let accepted_dependency_values_root_fragment =
        evidence.facts.accepted_dependency_values_root_fragment;
    let (policy_inputs, provider_schema_candidate) = build_path_schema_inputs(
        evidence,
        values_yaml_info,
        provider,
        resolve_policy,
        provider_schema_cache,
    );
    let schema = resolve_policy.resolve_schema_for_value_path(policy_inputs);
    let provider_schema_candidate =
        provider_schema_candidate.filter(|provider_schema| provider_schema.survives_as(&schema));

    ResolvedPathSchema {
        value_path,
        path_segments,
        schema,
        values_yaml_schema: values_yaml_info
            .map(|path_info| path_info.schema.clone())
            .unwrap_or_else(empty_schema),
        provider_schema_candidate,
        used_as_pathless_fragment,
        accepted_dependency_values_root_fragment,
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

fn build_path_schema_inputs(
    evidence: ContractPathSchemaEvidence,
    values_yaml_info: Option<&ValuesYamlPathInfo>,
    provider: &dyn ResourceSchemaOracle,
    resolve_policy: &ResolvePolicy,
    provider_schema_cache: &mut HashMap<
        ProviderSchemaLookupKey,
        Option<Arc<ProviderSchemaCandidate>>,
    >,
) -> (ValuePathSchemaInputs, Option<ProviderSchemaCandidate>) {
    let provider_schemas = provider_schemas_for_path_evidence(
        &evidence,
        provider,
        resolve_policy,
        provider_schema_cache,
    );
    let (provider_schema, provider_schema_candidate) = provider_schema_for_path(
        provider_schemas,
        metadata_schema(&evidence.metadata_field_kinds),
    );
    let values_yaml_facts =
        values_yaml_info.map_or_else(ValuesYamlPathFacts::absent, |path_info| path_info.facts());
    let facts = ValuePathSchemaFacts::new(evidence.facts, values_yaml_facts);
    let values_yaml_schema = values_yaml_info
        .map(|path_info| path_info.schema.clone())
        .unwrap_or_else(empty_schema);

    (
        ValuePathSchemaInputs {
            facts,
            provider_schema,
            values_yaml_schema,
            guard_predicate_schema: guard_predicate_schema(
                &evidence.value_path,
                &evidence.guard_predicates,
                resolve_policy,
            ),
            type_hint_schema: type_hint_schema(&evidence.type_hints),
        },
        provider_schema_candidate,
    )
}

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

fn provider_schema_for_path(
    provider_schemas: Vec<Arc<ProviderSchemaCandidate>>,
    metadata_schema: Value,
) -> (Value, Option<ProviderSchemaCandidate>) {
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

    (
        merge_schema_list(vec![provider_schema, metadata_schema]),
        provider_schema_candidate,
    )
}

fn metadata_field_schema(field: MetadataFieldKind) -> Value {
    match field {
        MetadataFieldKind::StringMap => string_map_schema(),
        MetadataFieldKind::Name | MetadataFieldKind::Namespace => type_schema("string"),
    }
}

fn metadata_schema(field_kinds: &BTreeSet<MetadataFieldKind>) -> Value {
    if field_kinds.is_empty() {
        empty_schema()
    } else {
        merge_schema_list(
            field_kinds
                .iter()
                .copied()
                .map(metadata_field_schema)
                .collect(),
        )
    }
}

fn type_hint_schema(schema_types: &BTreeSet<String>) -> Value {
    if schema_types.is_empty() {
        return empty_schema();
    }

    merge_schema_list(
        schema_types
            .iter()
            .map(|schema_type| type_schema(schema_type))
            .collect(),
    )
}

fn guard_predicate_schema(
    value_path: &str,
    guard_predicates: &[helm_schema_ir::ConditionalGuard],
    resolve_policy: &ResolvePolicy,
) -> Value {
    merge_schema_list(
        guard_predicates
            .iter()
            .filter_map(|predicate| resolve_policy.guard_predicate_schema(value_path, predicate))
            .collect(),
    )
}

fn string_map_schema() -> Value {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("additionalProperties".to_string(), type_schema("string"));
    Value::Object(schema)
}
