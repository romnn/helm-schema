use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::{Map, Value};

use helm_schema_ir::{
    ContractPathSignals, ContractProjection, ContractUse, MetadataFieldKind, ValueKind,
};
use helm_schema_k8s::{K8sSchemaProvider, type_schema};

use crate::resolve_policy::ResolvePolicy;

pub(crate) struct UseSignals {
    pub(crate) referenced_value_paths: BTreeSet<String>,
    pub(crate) ranged_value_paths: BTreeSet<String>,
    pub(crate) value_paths_used_as_fragment: BTreeSet<String>,
    pub(crate) partial_scalar_value_paths: BTreeSet<String>,
    pub(crate) provider_schemas_by_value_path: BTreeMap<String, Vec<Arc<Value>>>,
    pub(crate) metadata_schemas_by_value_path: BTreeMap<String, Vec<Value>>,
    pub(crate) guard_constraints_by_value_path: BTreeMap<String, Vec<Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProviderSchemaLookupKey {
    resource: helm_schema_ir::ResourceRef,
    path: helm_schema_ir::YamlPath,
    kind: ValueKind,
}

#[tracing::instrument(skip_all, fields(uses = contract_projection.uses().len()))]
pub(crate) fn collect_use_signals(
    contract_projection: &ContractProjection,
    provider: &dyn K8sSchemaProvider,
) -> UseSignals {
    let uses = contract_projection.uses();
    let resolve_policy = ResolvePolicy::default();
    let ContractPathSignals {
        referenced_value_paths,
        ranged_value_paths,
        value_paths_used_as_fragment,
        partial_scalar_value_paths,
        guard_constraints_by_value_path,
        metadata_fields_by_value_path,
    } = contract_projection.path_signals();
    let mut provider_schemas_by_value_path: BTreeMap<String, Vec<Arc<Value>>> = BTreeMap::new();
    let metadata_schemas_by_value_path = metadata_fields_by_value_path
        .into_iter()
        .map(|(path, fields)| {
            (
                path,
                fields
                    .into_iter()
                    .map(metadata_field_schema)
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let guard_constraints_by_value_path = guard_constraints_by_value_path
        .into_iter()
        .filter_map(|(path, constraints)| {
            let schemas: Vec<Value> = constraints
                .iter()
                .filter_map(|constraint| resolve_policy.guard_constraint_schema(constraint))
                .collect();
            (!schemas.is_empty()).then_some((path, schemas))
        })
        .collect();
    let mut provider_schema_cache: HashMap<ProviderSchemaLookupKey, Option<Arc<Value>>> =
        HashMap::new();

    for contract_use in uses {
        if contract_use.source_expr.trim().is_empty() {
            continue;
        }

        if contract_use.kind != ValueKind::PartialScalar
            && !contract_use.path.0.is_empty()
            && let Some(resource) = &contract_use.resource
        {
            let lookup_key = ProviderSchemaLookupKey {
                resource: resource.clone(),
                path: contract_use.path.clone(),
                kind: contract_use.kind,
            };
            let schema = match provider_schema_cache.entry(lookup_key) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.get().clone(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let schema = lookup_provider_schema(provider, contract_use, &resolve_policy);
                    entry.insert(schema.clone());
                    schema
                }
            };
            if let Some(schema) = schema {
                let provider_schemas = provider_schemas_by_value_path
                    .entry(contract_use.source_expr.clone())
                    .or_default();
                if !provider_schemas
                    .iter()
                    .any(|existing| Arc::ptr_eq(existing, &schema))
                {
                    provider_schemas.push(schema);
                }
            }
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
        resource_kind = contract_use
            .resource
            .as_ref()
            .map(|resource| resource.kind.as_str())
            .unwrap_or(""),
        resource_api_version = contract_use
            .resource
            .as_ref()
            .map(|resource| resource.api_version.as_str())
            .unwrap_or(""),
        path_len = contract_use.path.0.len(),
    )
)]
fn lookup_provider_schema(
    provider: &dyn K8sSchemaProvider,
    contract_use: &ContractUse,
    resolve_policy: &ResolvePolicy,
) -> Option<Arc<Value>> {
    provider
        .schema_for_use(contract_use)
        .and_then(|schema| resolve_policy.provider_schema_for_value_use(schema, contract_use))
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
