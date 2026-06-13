use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::{Map, Value};

use helm_schema_ir::{ContractPathSignals, MetadataFieldKind, ProviderSchemaUse};
use helm_schema_k8s::{K8sSchemaProvider, type_schema};

use crate::resolve_policy::ResolvePolicy;

pub(crate) struct UseSignals {
    pub(crate) referenced_value_paths: BTreeSet<String>,
    pub(crate) provider_schemas_by_value_path: BTreeMap<String, Vec<Arc<Value>>>,
    pub(crate) metadata_schemas_by_value_path: BTreeMap<String, Vec<Value>>,
    pub(crate) guard_constraints_by_value_path: BTreeMap<String, Vec<Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProviderSchemaLookupKey {
    resource: helm_schema_ir::ResourceRef,
    path: helm_schema_ir::YamlPath,
    kind: helm_schema_ir::ValueKind,
    is_self_range_collection: bool,
}

#[tracing::instrument(skip_all, fields(provider_uses = provider_schema_uses.len()))]
pub(crate) fn collect_use_signals(
    path_signals: ContractPathSignals,
    provider_schema_uses: &[ProviderSchemaUse],
    provider: &dyn K8sSchemaProvider,
) -> UseSignals {
    let resolve_policy = ResolvePolicy::default();
    let ContractPathSignals {
        referenced_value_paths,
        guard_constraints_by_value_path,
        metadata_fields_by_value_path,
        ..
    } = path_signals;
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
                let schema = lookup_provider_schema(provider, &provider_use, &resolve_policy);
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

    UseSignals {
        referenced_value_paths,
        provider_schemas_by_value_path,
        metadata_schemas_by_value_path,
        guard_constraints_by_value_path,
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
    provider: &dyn K8sSchemaProvider,
    provider_use: &ProviderSchemaUse,
    resolve_policy: &ResolvePolicy,
) -> Option<Arc<Value>> {
    provider
        .schema_for_use(provider_use)
        .and_then(|schema| resolve_policy.provider_schema_for_value_use(schema, provider_use))
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
