mod merge;
mod path_resolver;
mod path_schema;
mod provider_definitions;
mod provider_schema;
pub mod required_inference;
mod resolve_policy;
mod schema_model;
mod schema_tree;
mod use_signals;
mod values_yaml;

use std::collections::BTreeMap;

use helm_schema_core::ResourceSchemaOracle;
use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{
    ConditionalGuard, ConditionalPathOverlay, ContractSchemaSignals, ContractValuePathFacts,
};

use path_resolver::PathSchemaResolver;
use provider_definitions::ProviderSchemaDefinitions;
use schema_tree::{
    apply_values_descriptions, ensure_schema_node_at_path_segments, insert_schema_at_path_segments,
    object_schema,
};
use use_signals::{UseSignals, collect_use_signals};

// ---------------------------------------------------------------------------
// Core generation logic
// ---------------------------------------------------------------------------

/// Inputs for JSON Schema generation from the current contract schema signals.
///
/// The generated schema is derived from the contract-layer signal bundle plus
/// optional structural signals collected by earlier analysis phases.
/// Values-file descriptions are metadata only: they are applied only to schema
/// nodes that already exist from template or values evidence.
pub struct ValuesSchemaInput<'a> {
    pub contract_schema_signals: &'a ContractSchemaSignals,
    pub provider: &'a dyn ResourceSchemaOracle,
    pub values_yaml: Option<&'a str>,
    pub type_hints: Option<&'a BTreeMap<String, Vec<Value>>>,
    pub values_descriptions: Option<&'a BTreeMap<String, String>>,
}

impl<'a> ValuesSchemaInput<'a> {
    pub fn new(
        contract_schema_signals: &'a ContractSchemaSignals,
        provider: &'a dyn ResourceSchemaOracle,
    ) -> Self {
        Self {
            contract_schema_signals,
            provider,
            values_yaml: None,
            type_hints: None,
            values_descriptions: None,
        }
    }

    pub fn with_values_yaml(mut self, values_yaml: Option<&'a str>) -> Self {
        self.values_yaml = values_yaml;
        self
    }

    pub fn with_type_hints(mut self, type_hints: &'a BTreeMap<String, Vec<Value>>) -> Self {
        self.type_hints = Some(type_hints);
        self
    }

    pub fn with_values_descriptions(
        mut self,
        values_descriptions: &'a BTreeMap<String, String>,
    ) -> Self {
        self.values_descriptions = Some(values_descriptions);
        self
    }
}

/// Generate a JSON Schema with chart-authored values-file descriptions.
///
/// The output schema has no `required` arrays inferred by helm-schema; callers
/// that want that behaviour layer [`required_inference::apply_required_inference`]
/// on top of the returned schema. Keeping required-inference outside this
/// function isolates a heuristic feature from the core schema-generation
/// pipeline.
#[tracing::instrument(skip_all)]
pub fn generate_values_schema(input: ValuesSchemaInput<'_>) -> Value {
    let empty_type_hints = BTreeMap::new();
    let type_hints = input.type_hints.unwrap_or(&empty_type_hints);
    let empty_values_descriptions = BTreeMap::new();
    let values_descriptions = input
        .values_descriptions
        .unwrap_or(&empty_values_descriptions);

    let path_signals = input.contract_schema_signals.path_signals.clone();
    let mut value_path_facts = input.contract_schema_signals.value_path_facts.clone();
    let mut signals = collect_use_signals(
        path_signals,
        &input.contract_schema_signals.provider_schema_uses,
        input.provider,
    );
    signals
        .referenced_value_paths
        .extend(type_hints.keys().cloned());
    mark_type_hint_descendant_facts(&mut value_path_facts, type_hints.keys());

    let values_yaml_doc = input
        .values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);

    let root_schema = build_root_schema(
        signals,
        &value_path_facts,
        &values_yaml_doc,
        type_hints,
        values_descriptions,
        &input.contract_schema_signals.conditional_path_overlays,
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

fn mark_type_hint_descendant_facts<'a>(
    value_path_facts: &mut BTreeMap<String, ContractValuePathFacts>,
    paths: impl IntoIterator<Item = &'a String>,
) {
    for path in paths {
        let mut segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            value_path_facts
                .entry(segments.join("."))
                .or_default()
                .has_referenced_descendants = true;
        }
    }
}

#[tracing::instrument(skip_all)]
fn build_root_schema(
    signals: UseSignals,
    value_path_facts: &BTreeMap<String, ContractValuePathFacts>,
    values_yaml_doc: &YamlValue,
    type_hints: &BTreeMap<String, Vec<Value>>,
    values_descriptions: &BTreeMap<String, String>,
    conditional_path_overlays: &[ConditionalPathOverlay],
) -> Value {
    let mut root_schema = object_schema(Map::new());
    let path_resolver =
        PathSchemaResolver::new(signals, value_path_facts, values_yaml_doc, type_hints);
    let mut resolved_paths = path_resolver.resolve_all();
    let provider_definitions =
        ProviderSchemaDefinitions::from_resolved_paths(&mut resolved_paths, values_descriptions);

    let conditional_schemas =
        collect_conditional_schemas(&resolved_paths, conditional_path_overlays);
    let conditional_targets: std::collections::BTreeSet<&str> = conditional_schemas
        .iter()
        .map(|conditional| conditional.target_value_path.as_str())
        .collect();

    for resolved_path in resolved_paths {
        let schema = if conditional_targets.contains(resolved_path.value_path.as_str()) {
            crate::schema_model::empty_schema()
        } else {
            resolved_path.schema
        };
        insert_schema_at_path_segments(&mut root_schema, &resolved_path.path_segments, schema);
    }

    append_conditional_schemas(&mut root_schema, conditional_schemas);

    provider_definitions.insert_into_root(&mut root_schema);
    apply_values_descriptions(&mut root_schema, values_descriptions);

    root_schema
}

struct ConditionalResolvedSchema {
    target_value_path: String,
    ancestor_segments: Vec<String>,
    relative_target_segments: Vec<String>,
    guards: Vec<ConditionalGuard>,
    target_schema: Value,
}

fn collect_conditional_schemas(
    resolved_paths: &[path_resolver::ResolvedPathSchema],
    overlays: &[ConditionalPathOverlay],
) -> Vec<ConditionalResolvedSchema> {
    let resolved_by_path = resolved_paths
        .iter()
        .map(|resolved| (resolved.value_path.as_str(), &resolved.schema))
        .collect::<BTreeMap<_, _>>();

    overlays
        .iter()
        .filter_map(|overlay| {
            let resolved = resolved_paths
                .iter()
                .find(|resolved| resolved.value_path == overlay.target_value_path)?;
            if resolved.values_yaml_has_schema_evidence {
                return None;
            }
            let target_segments = split_value_path(&overlay.target_value_path);
            let ancestor_segments =
                conditional_ancestor_segments(&target_segments, &overlay.guards);
            let target_schema =
                (*resolved_by_path.get(overlay.target_value_path.as_str())?).clone();
            guards_supported_for_conditional_lowering(&overlay.guards, &resolved_by_path).then(
                || ConditionalResolvedSchema {
                    target_value_path: overlay.target_value_path.clone(),
                    relative_target_segments: target_segments[ancestor_segments.len()..].to_vec(),
                    ancestor_segments,
                    guards: overlay.guards.clone(),
                    target_schema,
                },
            )
        })
        .collect()
}

fn conditional_ancestor_segments(
    target_segments: &[String],
    guards: &[ConditionalGuard],
) -> Vec<String> {
    let mut shared_prefix = target_segments.to_vec();
    let mut guard_paths = Vec::new();
    for guard in guards {
        collect_guard_paths(guard, &mut guard_paths);
    }
    for guard_path in guard_paths {
        shared_prefix.truncate(common_prefix_len(&shared_prefix, &guard_path));
    }
    shared_prefix
}

fn guards_supported_for_conditional_lowering(
    guards: &[ConditionalGuard],
    resolved_by_path: &BTreeMap<&str, &Value>,
) -> bool {
    !guards.is_empty()
        && guards.iter().all(|guard| match guard {
            ConditionalGuard::Truthy { path } => resolved_by_path
                .get(path.as_str())
                .is_some_and(|schema| schema_is_boolean_like(schema)),
            ConditionalGuard::Eq { .. } | ConditionalGuard::TypeIs { .. } => true,
            ConditionalGuard::Not(inner) => guards_supported_for_conditional_lowering(
                std::slice::from_ref(inner),
                resolved_by_path,
            ),
            ConditionalGuard::AnyOf(guards) => {
                guards_supported_for_conditional_lowering(guards, resolved_by_path)
            }
        })
}

fn schema_is_boolean_like(schema: &Value) -> bool {
    crate::schema_model::schema_allows_scalar_type(schema, "boolean")
        && !crate::schema_model::schema_allows_scalar_type(schema, "string")
        && !crate::schema_model::schema_allows_scalar_type(schema, "integer")
        && !crate::schema_model::schema_allows_scalar_type(schema, "number")
        && !crate::schema_model::schema_allows_scalar_type(schema, "object")
        && !crate::schema_model::schema_allows_scalar_type(schema, "array")
}

fn append_conditional_schemas(
    root_schema: &mut Value,
    conditionals: Vec<ConditionalResolvedSchema>,
) {
    if conditionals.is_empty() {
        return;
    }

    for conditional in conditionals {
        let condition =
            build_condition_fragment(&conditional.guards, &conditional.ancestor_segments);
        let then_schema = build_target_fragment(
            &conditional.relative_target_segments,
            conditional.target_schema,
        );
        let ancestor =
            ensure_schema_node_at_path_segments(root_schema, &conditional.ancestor_segments);
        append_conditional_entry(ancestor, condition, then_schema);
    }
}

fn append_conditional_entry(node: &mut Value, condition: Value, then_schema: Value) {
    let Value::Object(object) = node else {
        return;
    };
    let all_of = object
        .entry("allOf".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(entries) = all_of else {
        return;
    };
    entries.push(Value::Object(
        [
            ("if".to_string(), condition),
            ("then".to_string(), then_schema),
        ]
        .into_iter()
        .collect(),
    ));
}

fn build_condition_fragment(guards: &[ConditionalGuard], ancestor_segments: &[String]) -> Value {
    let mut clauses = guards
        .iter()
        .filter_map(|guard| build_single_condition_fragment(guard, ancestor_segments))
        .collect::<Vec<_>>();

    if clauses.len() == 1 {
        clauses.pop().unwrap_or(Value::Object(Map::new()))
    } else {
        Value::Object(
            [("allOf".to_string(), Value::Array(clauses))]
                .into_iter()
                .collect(),
        )
    }
}

fn build_single_condition_fragment(
    guard: &ConditionalGuard,
    ancestor_segments: &[String],
) -> Option<Value> {
    match guard {
        ConditionalGuard::Truthy { path } => build_leaf_condition_fragment(
            path,
            ancestor_segments,
            Value::Object(
                [("const".to_string(), Value::Bool(true))]
                    .into_iter()
                    .collect(),
            ),
        ),
        ConditionalGuard::Eq { path, value } => build_leaf_condition_fragment(
            path,
            ancestor_segments,
            Value::Object(
                [(
                    "enum".to_string(),
                    Value::Array(vec![Value::String(value.clone())]),
                )]
                .into_iter()
                .collect(),
            ),
        ),
        ConditionalGuard::TypeIs { path, schema_type } => build_leaf_condition_fragment(
            path,
            ancestor_segments,
            Value::Object(
                [("type".to_string(), Value::String(schema_type.clone()))]
                    .into_iter()
                    .collect(),
            ),
        ),
        ConditionalGuard::Not(inner) => Some(Value::Object(
            [(
                "not".to_string(),
                build_single_condition_fragment(inner, ancestor_segments)?,
            )]
            .into_iter()
            .collect(),
        )),
        ConditionalGuard::AnyOf(guards) => {
            let mut clauses = guards
                .iter()
                .filter_map(|guard| build_single_condition_fragment(guard, ancestor_segments))
                .collect::<Vec<_>>();
            if clauses.is_empty() {
                None
            } else if clauses.len() == 1 {
                clauses.pop()
            } else {
                Some(Value::Object(
                    [("anyOf".to_string(), Value::Array(clauses))]
                        .into_iter()
                        .collect(),
                ))
            }
        }
    }
}

fn build_leaf_condition_fragment(
    value_path: &str,
    ancestor_segments: &[String],
    leaf_schema: Value,
) -> Option<Value> {
    let segments = split_value_path(value_path);
    let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
    if relative_segments.is_empty() {
        Some(leaf_schema)
    } else {
        build_required_condition_fragment(&relative_segments, leaf_schema)
    }
}

fn build_required_condition_fragment(
    path_segments: &[String],
    leaf_schema: Value,
) -> Option<Value> {
    let (head, tail) = path_segments.split_first()?;
    let mut object = Map::new();
    object.insert("type".to_string(), Value::String("object".to_string()));
    object.insert(
        "required".to_string(),
        Value::Array(vec![Value::String(head.clone())]),
    );
    let child = if tail.is_empty() {
        leaf_schema
    } else {
        build_required_condition_fragment(tail, leaf_schema)?
    };
    object.insert(
        "properties".to_string(),
        Value::Object(Map::from_iter([(head.clone(), child)])),
    );
    Some(Value::Object(object))
}

fn build_target_fragment(path_segments: &[String], leaf_schema: Value) -> Value {
    let Some((head, tail)) = path_segments.split_first() else {
        return leaf_schema;
    };

    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            (
                "properties".to_string(),
                Value::Object(Map::from_iter([(
                    head.clone(),
                    if tail.is_empty() {
                        leaf_schema
                    } else {
                        build_target_fragment(tail, leaf_schema)
                    },
                )])),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn split_value_path(path: &str) -> Vec<String> {
    path.split('.')
        .filter(|segment| !segment.is_empty())
        .map(std::string::ToString::to_string)
        .collect()
}

fn collect_guard_paths(guard: &ConditionalGuard, paths: &mut Vec<Vec<String>>) {
    match guard {
        ConditionalGuard::Truthy { path }
        | ConditionalGuard::Eq { path, .. }
        | ConditionalGuard::TypeIs { path, .. } => paths.push(split_value_path(path)),
        ConditionalGuard::Not(inner) => collect_guard_paths(inner, paths),
        ConditionalGuard::AnyOf(guards) => {
            for guard in guards {
                collect_guard_paths(guard, paths);
            }
        }
    }
}

fn common_prefix_len(left: &[String], right: &[String]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

fn strip_ancestor_prefix(
    path_segments: &[String],
    ancestor_segments: &[String],
) -> Option<Vec<String>> {
    path_segments
        .starts_with(ancestor_segments)
        .then(|| path_segments[ancestor_segments.len()..].to_vec())
}

#[cfg(test)]
mod tests;
