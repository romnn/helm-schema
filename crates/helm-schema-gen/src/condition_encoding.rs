use helm_schema_core::{ConditionalGuard, GuardValue};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::schema_model::guard_value_to_json;
use crate::schema_node::{JsonSchemaType, SchemaNode};
use crate::split_value_path;
use crate::values_yaml::yaml_value_at_path;

pub(crate) fn build_condition_clauses(
    guards: &[ConditionalGuard],
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
) -> Vec<SchemaNode> {
    guards
        .iter()
        .filter_map(|guard| {
            build_single_condition_fragment(guard, ancestor_segments, values_yaml_doc)
        })
        .collect()
}

/// Pass-level memo for guard-condition fragments: big charts repeat the same
/// activation and branch guards across hundreds of arms, and rebuilding each
/// structural encoding dominates arm emission.
pub(crate) type ConditionFragmentCache =
    std::collections::BTreeMap<(Vec<String>, ConditionalGuard), Option<SchemaNode>>;

pub(crate) fn build_condition_clauses_cached(
    guards: &[ConditionalGuard],
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
    cache: &mut ConditionFragmentCache,
) -> Vec<SchemaNode> {
    guards
        .iter()
        .filter_map(|guard| {
            cache
                .entry((ancestor_segments.to_vec(), guard.clone()))
                .or_insert_with(|| {
                    build_single_condition_fragment(guard, ancestor_segments, values_yaml_doc)
                })
                .clone()
        })
        .collect()
}

fn build_single_condition_fragment(
    guard: &ConditionalGuard,
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
) -> Option<SchemaNode> {
    match guard {
        ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
            let declared = yaml_value_at_path(values_yaml_doc, path);
            // Helm COALESCES mapping values with a declared mapping default
            // instead of replacing it: under a non-empty mapping default,
            // any object the user writes (even `{}`) merges into that
            // default and renders truthy. The document-level test must
            // accept every object then, or its negation would reject
            // documents the chart renders fine. (Explicitly nulling out
            // every default key still merges to an empty — falsy — mapping;
            // that residue stays unmodeled, in the permissive direction.)
            let default_is_nonempty_mapping =
                matches!(declared, Some(YamlValue::Mapping(mapping)) if !mapping.is_empty());
            let truthy = if default_is_nonempty_mapping {
                SchemaNode::any_of(vec![SchemaNode::object(), helm_truthy_condition_schema()])
            } else {
                helm_truthy_condition_schema()
            };
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                truthy,
                declared.is_some_and(yaml_value_is_truthy),
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
            guard_value_enum_schema(value).map(SchemaNode::not)?,
            !guard_value_matches_optional_yaml(value, yaml_value_at_path(values_yaml_doc, path)),
        ),
        ConditionalGuard::Absent { path } => {
            let segments = split_value_path(path);
            let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
            if relative_segments.is_empty() {
                return None;
            }
            // Render-time absence after coalescing with declared defaults:
            // an explicit `null` deletes the key (helm null-deletion), and
            // a missing key stays absent only when the chart declares no
            // (non-null) default to fill it.
            let explicit_null = build_required_condition_fragment(
                &relative_segments,
                SchemaNode::enum_values(vec![Value::Null]),
            )?;
            let declared_default_fills = yaml_value_at_path(values_yaml_doc, path)
                .is_some_and(|value| !matches!(value, YamlValue::Null));
            if declared_default_fills {
                Some(explicit_null)
            } else {
                let missing = SchemaNode::not(build_required_condition_fragment(
                    &relative_segments,
                    SchemaNode::empty(),
                )?);
                Some(SchemaNode::any_of(vec![missing, explicit_null]))
            }
        }
        ConditionalGuard::TypeIs { path, schema_type } => {
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::type_named(schema_type),
                yaml_value_at_path(values_yaml_doc, path)
                    .is_some_and(|value| matches_yaml_schema_type(value, schema_type)),
            )
        }
        ConditionalGuard::MatchesPattern { path, pattern } => {
            let default_matches = yaml_value_at_path(values_yaml_doc, path)
                .and_then(YamlValue::as_str)
                .is_some_and(|value| {
                    regex::Regex::new(pattern).is_ok_and(|regex| regex.is_match(value))
                });
            build_default_aware_leaf_condition_fragment(
                path,
                ancestor_segments,
                SchemaNode::foreign(serde_json::json!({
                    "pattern": pattern,
                    "type": "string",
                })),
                default_matches,
            )
        }
        ConditionalGuard::Not(inner) => Some(SchemaNode::not(build_single_condition_fragment(
            inner,
            ancestor_segments,
            values_yaml_doc,
        )?)),
        ConditionalGuard::AllOf(guards) => {
            let clauses = build_condition_clauses(guards, ancestor_segments, values_yaml_doc);
            (!clauses.is_empty()).then(|| SchemaNode::all_of(clauses))
        }
        ConditionalGuard::AnyOf(guards) => {
            let clauses = build_condition_clauses(guards, ancestor_segments, values_yaml_doc);
            (!clauses.is_empty()).then(|| SchemaNode::any_of(clauses))
        }
    }
}

/// Whether a guard (and every nested guard) produces a condition fragment.
/// `build_condition_clauses` silently FILTERS unencodable guards, which is
/// safe for `if/then` arms (a narrower `if` applies the arm less often) but
/// unsound for terminal `then: false` clauses, where a dropped conjunct
/// widens the rejected set. Mirrors `build_single_condition_fragment`.
pub(crate) fn guard_encodes_fully(
    guard: &ConditionalGuard,
    ancestor_segments: &[String],
    values_yaml_doc: &YamlValue,
) -> bool {
    match guard {
        ConditionalGuard::Not(inner) => {
            guard_encodes_fully(inner, ancestor_segments, values_yaml_doc)
        }
        ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
            !guards.is_empty()
                && guards
                    .iter()
                    .all(|guard| guard_encodes_fully(guard, ancestor_segments, values_yaml_doc))
        }
        other => {
            build_single_condition_fragment(other, ancestor_segments, values_yaml_doc).is_some()
        }
    }
}

fn guard_value_enum_schema(value: &GuardValue) -> Option<SchemaNode> {
    guard_value_to_json(value).map(|value| SchemaNode::enum_values(vec![value]))
}

fn build_default_aware_leaf_condition_fragment(
    value_path: &str,
    ancestor_segments: &[String],
    leaf_schema: SchemaNode,
    default_matches: bool,
) -> Option<SchemaNode> {
    let segments = split_value_path(value_path);
    let relative_segments = strip_ancestor_prefix(&segments, ancestor_segments)?;
    if relative_segments.is_empty() {
        return Some(leaf_schema);
    }
    let explicit = build_required_condition_fragment(&relative_segments, leaf_schema)?;
    if !default_matches {
        return Some(explicit);
    }
    let absent = build_required_condition_fragment(&relative_segments, SchemaNode::empty())
        .map(SchemaNode::not)?;
    Some(SchemaNode::any_of(vec![absent, explicit]))
}

pub(crate) fn value_references_helm_truthy(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, child)| {
            (key == "$ref"
                && child.as_str()
                    == Some(&format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}") as &str))
                || value_references_helm_truthy(child)
        }),
        Value::Array(items) => items.iter().any(value_references_helm_truthy),
        _ => false,
    }
}

/// Helm truthiness as one shared definition: every truthy/with condition
/// references it, which keeps the emitted `if` blocks small on charts with
/// many guarded renders.
pub(crate) const HELM_TRUTHY_DEFINITION_NAME: &str = "helm-truthy";

fn helm_truthy_condition_schema() -> SchemaNode {
    SchemaNode::foreign(serde_json::json!({
        "$ref": format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}")
    }))
}

pub(crate) fn helm_truthy_definition_schema() -> Value {
    SchemaNode::any_of(vec![
        SchemaNode::const_value(Value::Bool(true)),
        SchemaNode::typed(JsonSchemaType::Number).typed_keyword(
            "not",
            SchemaNode::const_value(Value::Number(0.into())).into_value(),
        ),
        SchemaNode::typed(JsonSchemaType::String)
            .typed_keyword("minLength", Value::Number(1.into())),
        SchemaNode::array().min_items(1),
        SchemaNode::object().min_properties(1),
    ])
    .into_value()
}

fn build_required_condition_fragment(
    path_segments: &[String],
    leaf_schema: SchemaNode,
) -> Option<SchemaNode> {
    let (head, tail) = path_segments.split_first()?;
    let child = if tail.is_empty() {
        leaf_schema
    } else {
        build_required_condition_fragment(tail, leaf_schema)?
    };
    Some(
        SchemaNode::object()
            .require(head.clone())
            .property(head.clone(), child),
    )
}

pub(crate) fn evaluate_guard_set_on_values(
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
        ConditionalGuard::MatchesPattern { path, pattern } => {
            let value = yaml_value_at_path(values_yaml_doc, path)?.as_str()?;
            regex::Regex::new(pattern)
                .ok()
                .map(|regex| regex.is_match(value))
        }
        ConditionalGuard::Not(inner) => {
            evaluate_guard_on_values(inner, values_yaml_doc).map(|v| !v)
        }
        ConditionalGuard::AllOf(guards) => evaluate_guard_set_on_values(guards, values_yaml_doc),
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

fn strip_ancestor_prefix(
    path_segments: &[String],
    ancestor_segments: &[String],
) -> Option<Vec<String>> {
    path_segments
        .starts_with(ancestor_segments)
        .then(|| path_segments[ancestor_segments.len()..].to_vec())
}
