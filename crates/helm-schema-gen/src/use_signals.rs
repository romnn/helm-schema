use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::{Map, Value};

use helm_schema_ir::{ContractProjection, Guard, ValueKind, ValueUse};
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

    for value_use in uses {
        if value_use.source_expr.trim().is_empty() {
            continue;
        }

        referenced_value_paths.insert(value_use.source_expr.clone());
        if value_use.kind == ValueKind::Fragment {
            value_paths_used_as_fragment.insert(value_use.source_expr.clone());
        }
        if value_use.kind == ValueKind::PartialScalar && !value_use.path.0.is_empty() {
            partial_scalar_value_paths.insert(value_use.source_expr.clone());
        }
        for guard in &value_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                referenced_value_paths.insert(path.to_string());
                if matches!(guard, Guard::Range { .. }) {
                    ranged_value_paths.insert(path.to_string());
                }

                if let Some(schema) = resolve_policy.guard_constraint_schema(guard) {
                    guard_constraints_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(schema);
                }
            }
        }

        if value_use.kind != ValueKind::PartialScalar
            && !value_use.path.0.is_empty()
            && let Some(resource) = &value_use.resource
        {
            let lookup_key = ProviderSchemaLookupKey {
                resource: resource.clone(),
                path: value_use.path.clone(),
                kind: value_use.kind,
            };
            let schema = match provider_schema_cache.entry(lookup_key) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.get().clone(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let schema = lookup_provider_schema(provider, value_use, &resolve_policy);
                    entry.insert(schema.clone());
                    schema
                }
            };
            if let Some(schema) = schema {
                let provider_schemas = provider_schemas_by_value_path
                    .entry(value_use.source_expr.clone())
                    .or_default();
                if !provider_schemas
                    .iter()
                    .any(|existing| Arc::ptr_eq(existing, &schema))
                {
                    provider_schemas.push(schema);
                }
            }
        }

        if let Some(schema) = infer_metadata_path_schema(&value_use.path.0) {
            metadata_schemas_by_value_path
                .entry(value_use.source_expr.clone())
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
        resource_kind = value_use
            .resource
            .as_ref()
            .map(|resource| resource.kind.as_str())
            .unwrap_or(""),
        resource_api_version = value_use
            .resource
            .as_ref()
            .map(|resource| resource.api_version.as_str())
            .unwrap_or(""),
        path_len = value_use.path.0.len(),
    )
)]
fn lookup_provider_schema(
    provider: &dyn K8sSchemaProvider,
    value_use: &ValueUse,
    resolve_policy: &ResolvePolicy,
) -> Option<Arc<Value>> {
    provider
        .schema_for_use(value_use)
        .and_then(|schema| resolve_policy.provider_schema_for_value_use(schema, value_use))
        .map(Arc::new)
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
