mod contract_evidence_index;
mod merge;
mod path_resolver;
mod path_schema;
mod provider_definitions;
mod provider_schema;
pub mod required_inference;
mod resolve_policy;
mod schema_model;
mod schema_tree;
mod values_yaml;

use std::collections::BTreeMap;

use helm_schema_core::ResourceSchemaOracle;
use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{ConditionalGuard, ConditionalPathOverlay, ContractSchemaSignals, GuardValue};

use merge::union_schema_list;
use path_resolver::PathSchemaResolver;
use provider_definitions::ProviderSchemaDefinitions;
use schema_model::{guard_value_to_json, schema_allows_type, type_schema};
use schema_tree::{
    apply_values_descriptions, ensure_schema_node_at_path_segments, insert_schema_at_path_segments,
    object_schema, open_array_schema, open_object_schema,
};

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
            values_descriptions: None,
        }
    }

    pub fn with_values_yaml(mut self, values_yaml: Option<&'a str>) -> Self {
        self.values_yaml = values_yaml;
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
    let empty_values_descriptions = BTreeMap::new();
    let values_descriptions = input
        .values_descriptions
        .unwrap_or(&empty_values_descriptions);

    let values_yaml_doc = input
        .values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);

    let root_schema = build_root_schema(
        input.contract_schema_signals,
        &values_yaml_doc,
        values_descriptions,
        input.provider,
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

#[tracing::instrument(skip_all)]
fn build_root_schema(
    contract_schema_signals: &ContractSchemaSignals,
    values_yaml_doc: &YamlValue,
    values_descriptions: &BTreeMap<String, String>,
    provider: &dyn ResourceSchemaOracle,
) -> Value {
    let mut root_schema = object_schema(Map::new());
    let path_resolver = PathSchemaResolver::new(contract_schema_signals, values_yaml_doc, provider);
    let mut resolved_paths = path_resolver.resolve_all();
    let provider_definitions =
        ProviderSchemaDefinitions::from_resolved_paths(&mut resolved_paths, values_descriptions);

    let conditional_schemas = collect_conditional_schemas(
        &resolved_paths,
        contract_schema_signals.conditional_path_overlays(),
        values_yaml_doc,
        provider,
    );
    let conditional_targets = summarize_conditional_targets(&conditional_schemas);

    for resolved_path in resolved_paths {
        let schema = match conditional_targets.get(resolved_path.value_path.as_str()) {
            Some(target) if target.preserve_base_schema => resolved_path.schema,
            Some(target) if target.open_fragment_base_schema => {
                open_fragment_base_schema(&resolved_path.schema)
            }
            Some(_) => crate::schema_model::empty_schema(),
            None => resolved_path.schema,
        };
        insert_schema_at_path_segments(&mut root_schema, &resolved_path.path_segments, schema);
    }

    append_conditional_schemas(&mut root_schema, conditional_schemas, values_yaml_doc);

    provider_definitions.insert_into_root(&mut root_schema);
    apply_values_descriptions(&mut root_schema, values_descriptions);

    root_schema
}

#[derive(Debug, Clone, Copy)]
struct ConditionalTargetSummary {
    preserve_base_schema: bool,
    open_fragment_base_schema: bool,
}

fn summarize_conditional_targets(
    conditionals: &[ConditionalResolvedSchema],
) -> BTreeMap<&str, ConditionalTargetSummary> {
    let mut targets = BTreeMap::new();
    for conditional in conditionals {
        let entry = targets
            .entry(conditional.target_value_path.as_str())
            .or_insert(ConditionalTargetSummary {
                preserve_base_schema: false,
                open_fragment_base_schema: true,
            });
        entry.preserve_base_schema |= conditional.preserve_base_schema;
        if !conditional.preserve_base_schema {
            entry.open_fragment_base_schema &= conditional.target_is_fragment;
        }
    }
    targets
}

fn open_fragment_base_schema(resolved_schema: &Value) -> Value {
    let mut schemas = Vec::new();
    if schema_allows_type(resolved_schema, "object") {
        schemas.push(open_object_schema());
    }
    if schema_allows_type(resolved_schema, "array") {
        schemas.push(open_array_schema());
    }
    if schema_allows_type(resolved_schema, "null") {
        schemas.push(type_schema("null"));
    }

    match schemas.len() {
        0 => crate::schema_model::empty_schema(),
        1 => schemas
            .pop()
            .expect("single open fragment schema should be present"),
        _ => union_schema_list(schemas),
    }
}

struct ConditionalResolvedSchema {
    target_value_path: String,
    ancestor_segments: Vec<String>,
    relative_target_segments: Vec<String>,
    guards: Vec<ConditionalGuard>,
    target_schema: Value,
    preserve_base_schema: bool,
    target_is_fragment: bool,
}

fn collect_conditional_schemas(
    resolved_paths: &[path_resolver::ResolvedPathSchema],
    overlays: &[ConditionalPathOverlay],
    values_yaml_doc: &YamlValue,
    provider: &dyn ResourceSchemaOracle,
) -> Vec<ConditionalResolvedSchema> {
    let resolved_by_path = resolved_paths
        .iter()
        .map(|resolved| (resolved.value_path.as_str(), &resolved.schema))
        .collect::<BTreeMap<_, _>>();

    overlays
        .iter()
        .filter_map(|overlay| {
            resolved_paths
                .iter()
                .find(|resolved| resolved.value_path == overlay.target_value_path)?;
            let target_segments = split_value_path(&overlay.target_value_path);
            let ancestor_segments =
                conditional_ancestor_segments(&target_segments, &overlay.guards);
            let target_schema = conditional_target_schema(
                overlay,
                values_yaml_doc,
                provider,
                resolved_by_path
                    .get(overlay.target_value_path.as_str())
                    .cloned()
                    .cloned(),
            );
            guards_supported_for_conditional_lowering(&overlay.guards, &resolved_by_path).then(
                || ConditionalResolvedSchema {
                    target_value_path: overlay.target_value_path.clone(),
                    relative_target_segments: target_segments[ancestor_segments.len()..].to_vec(),
                    ancestor_segments,
                    guards: overlay.guards.clone(),
                    target_schema,
                    preserve_base_schema: overlay.preserve_base_schema,
                    target_is_fragment: overlay.evidence.facts.used_as_fragment,
                },
            )
        })
        .filter(|conditional| !crate::schema_model::is_empty_schema(&conditional.target_schema))
        .collect()
}

fn conditional_target_schema(
    overlay: &ConditionalPathOverlay,
    values_yaml_doc: &YamlValue,
    provider: &dyn ResourceSchemaOracle,
    resolved_fallback: Option<Value>,
) -> Value {
    let branch_schema = resolve_overlay_target_schema(overlay, provider)
        .or(resolved_fallback.clone())
        .unwrap_or_else(crate::schema_model::empty_schema);

    let Some(active_by_defaults) = evaluate_guard_set_on_values(&overlay.guards, values_yaml_doc)
    else {
        return branch_schema;
    };
    if !active_by_defaults {
        if let Some(fallback) = resolved_fallback
            && is_placeholder_fragment_object_schema(&branch_schema)
            && !is_placeholder_fragment_object_schema(&fallback)
        {
            return fallback;
        }
        return branch_schema;
    }

    let Some(default_value) = yaml_value_at_path(values_yaml_doc, &overlay.target_value_path)
    else {
        return branch_schema;
    };
    let Ok(default_value) = serde_json::to_value(default_value) else {
        return branch_schema;
    };
    if schema_accepts_json_value(&branch_schema, &default_value) {
        branch_schema
    } else {
        resolved_fallback.unwrap_or(branch_schema)
    }
}

fn resolve_overlay_target_schema(
    overlay: &ConditionalPathOverlay,
    provider: &dyn ResourceSchemaOracle,
) -> Option<Value> {
    PathSchemaResolver::resolve_single_path_evidence(&overlay.evidence, &YamlValue::Null, provider)
        .map(|resolved| resolved.schema)
}

fn is_placeholder_fragment_object_schema(schema: &Value) -> bool {
    schema.as_object().is_some_and(|object| {
        object.get("type") == Some(&Value::String("object".to_string()))
            && object.get("additionalProperties") == Some(&Value::Object(Map::new()))
            && !object.contains_key("properties")
            && !object.contains_key("required")
    })
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
            ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => resolved_by_path
                .get(path.as_str())
                .is_some_and(|schema| schema_is_boolean_like(schema)),
            ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. } => true,
            ConditionalGuard::Not(inner) => guards_supported_for_conditional_lowering(
                std::slice::from_ref(inner),
                resolved_by_path,
            ),
            ConditionalGuard::AllOf(guards) => {
                guards_supported_for_conditional_lowering(guards, resolved_by_path)
            }
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
    values_yaml_doc: &YamlValue,
) {
    if conditionals.is_empty() {
        return;
    }

    for conditional in conditionals {
        let condition = build_condition_fragment(
            &conditional.guards,
            &conditional.ancestor_segments,
            values_yaml_doc,
        );
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

fn build_condition_fragment(
    guards: &[ConditionalGuard],
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
) -> Value {
    let mut clauses = guards
        .iter()
        .filter_map(|guard| {
            build_single_condition_fragment(guard, ancestor_segments, values_yaml_doc)
        })
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
    values_yaml_doc: &YamlValue,
) -> Option<Value> {
    match guard {
        ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                helm_truthy_condition_schema(),
                yaml_value_at_path(values_yaml_doc, path).is_some_and(yaml_value_is_truthy),
            )
        }
        ConditionalGuard::Eq { path, value } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            guard_value_enum_schema(value)?,
            guard_value_matches_optional_yaml(value, yaml_value_at_path(values_yaml_doc, path)),
        ),
        ConditionalGuard::NotEq { path, value } => build_default_aware_leaf_condition_fragment(
            path,
            ancestor_segments,
            Value::Object(
                [("not".to_string(), guard_value_enum_schema(value)?)]
                    .into_iter()
                    .collect(),
            ),
            !guard_value_matches_optional_yaml(value, yaml_value_at_path(values_yaml_doc, path)),
        ),
        ConditionalGuard::Absent { path } => {
            let segments = split_value_path(path);
            let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
            if relative_segments.is_empty() {
                None
            } else {
                build_required_condition_fragment(&relative_segments, Value::Object(Map::new()))
                    .map(|present| {
                        Value::Object([("not".to_string(), present)].into_iter().collect())
                    })
            }
        }
        ConditionalGuard::TypeIs { path, schema_type } => {
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                Value::Object(
                    [("type".to_string(), Value::String(schema_type.clone()))]
                        .into_iter()
                        .collect(),
                ),
                yaml_value_at_path(values_yaml_doc, path)
                    .is_some_and(|value| matches_yaml_schema_type(value, schema_type)),
            )
        }
        ConditionalGuard::Not(inner) => Some(Value::Object(
            [(
                "not".to_string(),
                build_single_condition_fragment(inner, ancestor_segments, values_yaml_doc)?,
            )]
            .into_iter()
            .collect(),
        )),
        ConditionalGuard::AllOf(guards) => {
            let mut clauses = guards
                .iter()
                .filter_map(|guard| {
                    build_single_condition_fragment(guard, ancestor_segments, values_yaml_doc)
                })
                .collect::<Vec<_>>();
            if clauses.is_empty() {
                None
            } else if clauses.len() == 1 {
                clauses.pop()
            } else {
                Some(Value::Object(
                    [("allOf".to_string(), Value::Array(clauses))]
                        .into_iter()
                        .collect(),
                ))
            }
        }
        ConditionalGuard::AnyOf(guards) => {
            let mut clauses = guards
                .iter()
                .filter_map(|guard| {
                    build_single_condition_fragment(guard, ancestor_segments, values_yaml_doc)
                })
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

fn guard_value_enum_schema(value: &GuardValue) -> Option<Value> {
    guard_value_to_json(value).map(|value| {
        Value::Object(
            [("enum".to_string(), Value::Array(vec![value]))]
                .into_iter()
                .collect(),
        )
    })
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

fn build_default_aware_leaf_condition_fragment(
    value_path: &str,
    ancestor_segments: &[String],
    leaf_schema: Value,
    default_matches: bool,
) -> Option<Value> {
    let explicit = build_leaf_condition_fragment(value_path, ancestor_segments, leaf_schema)?;
    if !default_matches {
        return Some(explicit);
    }

    let segments = split_value_path(value_path);
    let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
    if relative_segments.is_empty() {
        return Some(explicit);
    }
    let absent =
        build_required_condition_fragment(&relative_segments, Value::Object(Map::new()))
            .map(|present| Value::Object([("not".to_string(), present)].into_iter().collect()))?;
    Some(Value::Object(
        [("anyOf".to_string(), Value::Array(vec![absent, explicit]))]
            .into_iter()
            .collect(),
    ))
}

fn helm_truthy_condition_schema() -> Value {
    Value::Object(
        [(
            "anyOf".to_string(),
            Value::Array(vec![
                Value::Object(
                    [("const".to_string(), Value::Bool(true))]
                        .into_iter()
                        .collect(),
                ),
                Value::Object(
                    [
                        ("type".to_string(), Value::String("number".to_string())),
                        (
                            "not".to_string(),
                            Value::Object(
                                [("const".to_string(), Value::Number(0.into()))]
                                    .into_iter()
                                    .collect(),
                            ),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
                Value::Object(
                    [
                        ("type".to_string(), Value::String("string".to_string())),
                        ("minLength".to_string(), Value::Number(1.into())),
                    ]
                    .into_iter()
                    .collect(),
                ),
                Value::Object(
                    [
                        ("type".to_string(), Value::String("array".to_string())),
                        ("minItems".to_string(), Value::Number(1.into())),
                    ]
                    .into_iter()
                    .collect(),
                ),
                Value::Object(
                    [
                        ("type".to_string(), Value::String("object".to_string())),
                        ("minProperties".to_string(), Value::Number(1.into())),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ]),
        )]
        .into_iter()
        .collect(),
    )
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

fn evaluate_guard_set_on_values(
    guards: &[ConditionalGuard],
    values_yaml_doc: &YamlValue,
) -> Option<bool> {
    guards
        .iter()
        .map(|guard| evaluate_guard_on_values(guard, values_yaml_doc))
        .collect::<Option<Vec<_>>>()
        .map(|results| results.into_iter().all(|result| result))
}

fn evaluate_guard_on_values(guard: &ConditionalGuard, values_yaml_doc: &YamlValue) -> Option<bool> {
    match guard {
        ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
            Some(yaml_value_at_path(values_yaml_doc, path).is_some_and(yaml_value_is_truthy))
        }
        ConditionalGuard::Eq { path, value } => Some(guard_value_matches_optional_yaml(
            value,
            yaml_value_at_path(values_yaml_doc, path),
        )),
        ConditionalGuard::NotEq { path, value } => Some(!guard_value_matches_optional_yaml(
            value,
            yaml_value_at_path(values_yaml_doc, path),
        )),
        ConditionalGuard::Absent { path } => {
            Some(yaml_value_at_path(values_yaml_doc, path).is_none())
        }
        ConditionalGuard::TypeIs { path, schema_type } => {
            let Some(value) = yaml_value_at_path(values_yaml_doc, path) else {
                return Some(false);
            };
            Some(matches_yaml_schema_type(value, schema_type))
        }
        ConditionalGuard::Not(inner) => {
            evaluate_guard_on_values(inner, values_yaml_doc).map(|v| !v)
        }
        ConditionalGuard::AllOf(guards) => guards
            .iter()
            .map(|guard| evaluate_guard_on_values(guard, values_yaml_doc))
            .collect::<Option<Vec<_>>>()
            .map(|results| results.into_iter().all(|result| result)),
        ConditionalGuard::AnyOf(guards) => guards
            .iter()
            .map(|guard| evaluate_guard_on_values(guard, values_yaml_doc))
            .collect::<Option<Vec<_>>>()
            .map(|results| results.into_iter().any(|result| result)),
    }
}

fn guard_value_matches_optional_yaml(value: &GuardValue, yaml: Option<&YamlValue>) -> bool {
    let Some(yaml) = yaml else {
        return matches!(value, GuardValue::Null);
    };
    match value {
        GuardValue::String(expected) => yaml.as_str() == Some(expected.as_str()),
        GuardValue::Bool(expected) => yaml.as_bool() == Some(*expected),
        GuardValue::Int(expected) => {
            yaml.as_i64() == Some(*expected)
                || (*expected >= 0 && yaml.as_u64() == Some(*expected as u64))
        }
        GuardValue::Float(expected) => {
            let Some(expected) = expected.parse::<f64>().ok() else {
                return false;
            };
            yaml.as_f64() == Some(expected)
        }
        GuardValue::Null => matches!(yaml, YamlValue::Null),
    }
}

fn yaml_value_is_truthy(value: &YamlValue) -> bool {
    match value {
        YamlValue::Null => false,
        YamlValue::Bool(value) => *value,
        YamlValue::Number(value) => {
            value.as_i64().is_some_and(|value| value != 0)
                || value.as_u64().is_some_and(|value| value != 0)
                || value.as_f64().is_some_and(|value| value != 0.0)
        }
        YamlValue::String(value) => !value.is_empty(),
        YamlValue::Sequence(value) => !value.is_empty(),
        YamlValue::Mapping(value) => !value.is_empty(),
        YamlValue::Tagged(_) => false,
    }
}

fn matches_yaml_schema_type(value: &YamlValue, schema_type: &str) -> bool {
    match schema_type {
        "array" => matches!(value, YamlValue::Sequence(_)),
        "boolean" => value.as_bool().is_some(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.as_f64().is_some(),
        "object" => matches!(value, YamlValue::Mapping(_)),
        "string" => value.as_str().is_some(),
        _ => false,
    }
}

fn yaml_value_at_path<'a>(root: &'a YamlValue, value_path: &str) -> Option<&'a YamlValue> {
    let mut current = root;
    for segment in value_path.split('.').filter(|segment| !segment.is_empty()) {
        let YamlValue::Mapping(mapping) = current else {
            return None;
        };
        current = mapping.get(&YamlValue::String(segment.to_string()))?;
    }
    Some(current)
}

fn schema_accepts_json_value(schema: &Value, instance: &Value) -> bool {
    jsonschema::validator_for(schema)
        .map(|validator| validator.is_valid(instance))
        .unwrap_or(false)
}

fn collect_guard_paths(guard: &ConditionalGuard, paths: &mut Vec<Vec<String>>) {
    match guard {
        ConditionalGuard::Truthy { path }
        | ConditionalGuard::With { path }
        | ConditionalGuard::Eq { path, .. }
        | ConditionalGuard::NotEq { path, .. }
        | ConditionalGuard::Absent { path }
        | ConditionalGuard::TypeIs { path, .. } => paths.push(split_value_path(path)),
        ConditionalGuard::Not(inner) => collect_guard_paths(inner, paths),
        ConditionalGuard::AllOf(guards) => {
            for guard in guards {
                collect_guard_paths(guard, paths);
            }
        }
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
