use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use helm_schema_ir::{Guard, ValueKind, ValueUse};
use helm_schema_k8s::type_schema;

use crate::merge::{merge_two_schemas, union_schema_list};
use crate::schema_model::{
    empty_schema, empty_string_schema, is_empty_schema, is_fixed_object_schema,
    is_object_or_array_schema, is_open_string_map_schema, is_scalar_schema,
    schema_allows_scalar_type, schema_permits_empty_string, schema_type,
};
use crate::schema_tree::unknown_object_schema;

/// Generator-side policy for lowering semantic value uses into schema evidence.
///
/// The IR still crosses into the generator as [`ValueUse`] DTOs, but the
/// decisions about provider-schema domains and guard-derived constraints live
/// here rather than being spread across root-schema construction.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ResolvePolicy;

/// Inputs for one value-path schema decision.
///
/// These are the evidence streams collected for a single `.Values.*` path
/// before the policy decides which schemas to prefer, merge, or preserve.
pub(crate) struct ValuePathSchemaInputs {
    pub(crate) has_referenced_descendants: bool,
    pub(crate) used_as_fragment: bool,
    pub(crate) provider_schema: Value,
    pub(crate) values_yaml_schema: Value,
    pub(crate) guard_constraint_schema: Value,
    pub(crate) type_hint_schema: Value,
    pub(crate) preserve_empty_string_fallback: bool,
}

impl ResolvePolicy {
    pub(crate) fn provider_schema_for_value_use(
        &self,
        schema: Value,
        use_: &ValueUse,
    ) -> Option<Value> {
        match use_.kind {
            ValueKind::Fragment => Some(schema),
            ValueKind::PartialScalar => None,
            ValueKind::Scalar if self.is_self_range_collection_use(use_) => {
                restrict_schema_to_scalar_collection_domain(schema)
            }
            ValueKind::Scalar => restrict_schema_to_scalar_domain(schema),
        }
    }

    pub(crate) fn guard_constraint_schema(&self, guard: &Guard) -> Option<Value> {
        match guard {
            Guard::Eq { value, .. } => Some(Value::Object(
                [(
                    "anyOf".to_string(),
                    Value::Array(vec![
                        Value::Object(
                            [(
                                "enum".to_string(),
                                Value::Array(vec![Value::String(value.clone())]),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        type_schema("string"),
                    ]),
                )]
                .into_iter()
                .collect(),
            )),
            Guard::TypeIs { schema_type, .. } => match schema_type.as_str() {
                "array" | "boolean" | "integer" | "number" | "object" | "string" => {
                    Some(type_schema(schema_type))
                }
                _ => None,
            },
            Guard::Truthy { .. }
            | Guard::Not { .. }
            | Guard::Or { .. }
            | Guard::Range { .. }
            | Guard::With { .. }
            | Guard::Default { .. } => None,
        }
    }

    pub(crate) fn restrict_to_scalar_domain(&self, schema: Value) -> Option<Value> {
        restrict_schema_to_scalar_domain(schema)
    }

    pub(crate) fn resolve_schema_for_value_path(&self, input: ValuePathSchemaInputs) -> Value {
        let base = if !is_empty_schema(&input.provider_schema) {
            if is_empty_schema(&input.values_yaml_schema) {
                input.provider_schema
            } else {
                // Some charts use scalar "preset" values that are fed into helpers which
                // expand into full K8s objects in the rendered manifest (e.g. affinity presets).
                // In these cases the *input* type in values.yaml is the scalar, not the output
                // object type, so prefer the values.yaml scalar schema.
                if input.has_referenced_descendants
                    && is_fixed_object_schema(&input.values_yaml_schema)
                    && is_scalar_schema(&input.provider_schema)
                {
                    input.values_yaml_schema
                } else if input.used_as_fragment
                    && is_fixed_object_schema(&input.values_yaml_schema)
                    && is_open_string_map_schema(&input.provider_schema)
                {
                    input.provider_schema
                } else if input.used_as_fragment
                    && is_scalar_schema(&input.values_yaml_schema)
                    && is_object_or_array_schema(&input.provider_schema)
                {
                    input.values_yaml_schema
                } else if let Some(values_yaml_ty) = schema_type(&input.values_yaml_schema)
                    && is_scalar_schema(&input.values_yaml_schema)
                    && schema_allows_scalar_type(&input.provider_schema, values_yaml_ty)
                {
                    if input.preserve_empty_string_fallback
                        && values_yaml_ty == "string"
                        && !schema_permits_empty_string(&input.provider_schema)
                    {
                        union_schema_list(vec![input.provider_schema, empty_string_schema()])
                    } else {
                        input.provider_schema
                    }
                } else {
                    merge_two_schemas(input.provider_schema, input.values_yaml_schema)
                }
            }
        } else if !is_empty_schema(&input.values_yaml_schema) {
            input.values_yaml_schema
        } else if input.used_as_fragment {
            unknown_object_schema()
        } else {
            empty_schema()
        };

        let base = if is_empty_schema(&input.type_hint_schema) {
            base
        } else if is_empty_schema(&base) {
            input.type_hint_schema
        } else {
            merge_two_schemas(base, input.type_hint_schema)
        };

        if is_empty_schema(&input.guard_constraint_schema) {
            base
        } else if is_empty_schema(&base) {
            input.guard_constraint_schema
        } else {
            merge_two_schemas(base, input.guard_constraint_schema)
        }
    }

    /// Identify value paths for which an explicit `null` default in
    /// values.yaml is contractually valid according to the template control
    /// flow.
    ///
    /// A path qualifies when every observed use is null-tolerant and at least
    /// one rendered use provides non-null type evidence:
    ///
    /// - header-only guard/binding uses (`if` / `with` / `range` conditions)
    ///   are null-tolerant because Helm evaluates them against `nil` without
    ///   crashing;
    /// - rendered uses must sit under a self-guard that only renders the body
    ///   when the same value path is non-empty (`if .Values.X`, `with
    ///   .Values.X`, `range .Values.X`, `if eq .Values.X "literal"`, and
    ///   similar composed conditions that retain the per-path guard).
    ///
    /// Chart-level mutations on the values dict are handled at the IR layer by
    /// attaching `Guard::Default` to reads of the mutated path. This policy
    /// only consumes those structural guards; it does not infer nullability
    /// from a path being mentioned in any one default expression.
    pub(crate) fn nullable_value_paths(&self, uses: &[ValueUse]) -> BTreeSet<String> {
        let mut by_path: BTreeMap<&str, NullablePathInfo> = BTreeMap::new();
        for use_ in uses {
            if use_.source_expr.trim().is_empty() {
                continue;
            }
            let info = by_path.entry(use_.source_expr.as_str()).or_default();
            let has_self_range_guard = use_
                .guards
                .iter()
                .any(|guard| matches!(guard, Guard::Range { path } if path == &use_.source_expr));
            if !use_.path.0.is_empty() || has_self_range_guard || use_.kind == ValueKind::Fragment {
                info.has_render_use = true;
            }
            info.all_uses_nullable &= use_is_null_tolerant(use_);

            for guard in &use_.guards {
                if let Guard::Range { path } = guard
                    && !path.trim().is_empty()
                {
                    by_path.entry(path.as_str()).or_default().has_render_use = true;
                }
            }
        }
        by_path
            .into_iter()
            .filter_map(|(path, info)| {
                (info.has_render_use && info.all_uses_nullable).then(|| path.to_string())
            })
            .collect()
    }

    fn is_self_range_collection_use(&self, use_: &ValueUse) -> bool {
        use_.guards
            .iter()
            .any(|guard| matches!(guard, Guard::Range { path } if path == &use_.source_expr))
            && use_
                .path
                .0
                .last()
                .is_none_or(|segment| !segment.ends_with("[*]"))
    }
}

struct NullablePathInfo {
    has_render_use: bool,
    all_uses_nullable: bool,
}

impl Default for NullablePathInfo {
    fn default() -> Self {
        Self {
            has_render_use: false,
            all_uses_nullable: true,
        }
    }
}

fn use_is_null_tolerant(use_: &ValueUse) -> bool {
    if use_.path.0.is_empty() {
        return true;
    }

    use_.guards.iter().any(|guard| match guard {
        Guard::Truthy { path }
        | Guard::Eq { path, .. }
        | Guard::Range { path }
        | Guard::With { path }
        | Guard::Default { path } => path == &use_.source_expr,
        Guard::Not { .. } | Guard::Or { .. } | Guard::TypeIs { .. } => false,
    })
}

fn restrict_schema_to_scalar_domain(schema: Value) -> Option<Value> {
    match schema {
        Value::Object(mut obj) => {
            if let Some(variants) = obj.get("anyOf").and_then(Value::as_array).cloned() {
                return restrict_schema_union(
                    obj,
                    "anyOf",
                    variants,
                    restrict_schema_to_scalar_domain,
                );
            }
            if let Some(variants) = obj.get("oneOf").and_then(Value::as_array).cloned() {
                return restrict_schema_union(
                    obj,
                    "oneOf",
                    variants,
                    restrict_schema_to_scalar_domain,
                );
            }
            if let Some(variants) = obj.get("allOf").and_then(Value::as_array).cloned() {
                let mut scalar_variants = Vec::new();
                for variant in variants {
                    scalar_variants.push(restrict_schema_to_scalar_domain(variant)?);
                }
                obj.insert("allOf".to_string(), Value::Array(scalar_variants));
                return Some(Value::Object(obj));
            }

            if schema_allows_type_object(&obj, "array") {
                if let Some(items) = obj.remove("items") {
                    obj.insert(
                        "items".to_string(),
                        restrict_schema_to_scalar_domain(items)?,
                    );
                }
                obj.insert("type".to_string(), Value::String("array".to_string()));
                remove_object_keywords(&mut obj);
                return Some(Value::Object(obj));
            }

            match obj.get("type") {
                Some(Value::String(schema_type)) => {
                    if scalar_json_type(schema_type) {
                        Some(Value::Object(obj))
                    } else {
                        None
                    }
                }
                Some(Value::Array(schema_types)) => {
                    let scalar_types: Vec<Value> = schema_types
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|schema_type| scalar_json_type(schema_type))
                        .map(|schema_type| Value::String(schema_type.to_string()))
                        .collect();
                    if scalar_types.is_empty() {
                        return None;
                    }
                    if scalar_types.len() != schema_types.len() {
                        obj.insert("type".to_string(), Value::Array(scalar_types));
                        remove_non_scalar_keywords(&mut obj);
                    }
                    Some(Value::Object(obj))
                }
                _ if has_non_scalar_keywords(&obj) => None,
                _ => Some(Value::Object(obj)),
            }
        }
        other => Some(other),
    }
}

fn schema_allows_type_object(obj: &Map<String, Value>, expected: &str) -> bool {
    match obj.get("type") {
        Some(Value::String(schema_type)) => schema_type == expected,
        Some(Value::Array(schema_types)) => schema_types
            .iter()
            .filter_map(Value::as_str)
            .any(|schema_type| schema_type == expected),
        Some(_) => false,
        None if expected == "array" => has_array_keywords(obj),
        None => false,
    }
}

fn restrict_schema_to_scalar_collection_domain(schema: Value) -> Option<Value> {
    match schema {
        Value::Object(mut obj) => {
            if let Some(variants) = obj.get("anyOf").and_then(Value::as_array).cloned() {
                return restrict_schema_union(
                    obj,
                    "anyOf",
                    variants,
                    restrict_schema_to_scalar_collection_domain,
                );
            }
            if let Some(variants) = obj.get("oneOf").and_then(Value::as_array).cloned() {
                return restrict_schema_union(
                    obj,
                    "oneOf",
                    variants,
                    restrict_schema_to_scalar_collection_domain,
                );
            }
            if let Some(variants) = obj.get("allOf").and_then(Value::as_array).cloned() {
                let mut collection_variants = Vec::new();
                for variant in variants {
                    collection_variants.push(restrict_schema_to_scalar_collection_domain(variant)?);
                }
                obj.insert("allOf".to_string(), Value::Array(collection_variants));
                return Some(Value::Object(obj));
            }

            let is_array_schema = match obj.get("type") {
                Some(Value::String(schema_type)) => schema_type == "array",
                Some(Value::Array(schema_types)) => schema_types
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|schema_type| schema_type == "array"),
                Some(_) => false,
                None => has_array_keywords(&obj),
            };
            if !is_array_schema {
                return None;
            }

            if let Some(items) = obj.remove("items") {
                obj.insert(
                    "items".to_string(),
                    restrict_schema_to_scalar_domain(items)?,
                );
            }
            obj.insert("type".to_string(), Value::String("array".to_string()));
            remove_object_keywords(&mut obj);
            Some(Value::Object(obj))
        }
        _ => None,
    }
}

fn restrict_schema_union(
    obj: Map<String, Value>,
    keyword: &str,
    variants: Vec<Value>,
    restrict: fn(Value) -> Option<Value>,
) -> Option<Value> {
    let retained_variants: Vec<Value> = variants.into_iter().filter_map(restrict).collect();
    if retained_variants.is_empty() {
        return None;
    }

    let mut out = retain_schema_annotations(obj);
    out.insert(keyword.to_string(), Value::Array(retained_variants));
    Some(Value::Object(out))
}

fn retain_schema_annotations(obj: Map<String, Value>) -> Map<String, Value> {
    obj.into_iter()
        .filter(|(key, _)| is_schema_annotation_keyword(key))
        .collect()
}

fn is_schema_annotation_keyword(key: &str) -> bool {
    matches!(
        key,
        "description" | "title" | "default" | "examples" | "deprecated" | "readOnly" | "writeOnly"
    )
}

fn scalar_json_type(schema_type: &str) -> bool {
    matches!(
        schema_type,
        "string" | "number" | "integer" | "boolean" | "null"
    )
}

fn has_non_scalar_keywords(obj: &Map<String, Value>) -> bool {
    const NON_SCALAR_KEYWORDS: &[&str] = &[
        "additionalItems",
        "additionalProperties",
        "contains",
        "items",
        "maxItems",
        "maxProperties",
        "minItems",
        "minProperties",
        "patternProperties",
        "prefixItems",
        "properties",
        "propertyNames",
        "required",
        "uniqueItems",
    ];
    NON_SCALAR_KEYWORDS.iter().any(|key| obj.contains_key(*key))
}

fn has_array_keywords(obj: &Map<String, Value>) -> bool {
    const ARRAY_KEYWORDS: &[&str] = &[
        "additionalItems",
        "contains",
        "items",
        "maxItems",
        "minItems",
        "prefixItems",
        "uniqueItems",
    ];
    ARRAY_KEYWORDS.iter().any(|key| obj.contains_key(*key))
}

fn remove_non_scalar_keywords(obj: &mut Map<String, Value>) {
    const NON_SCALAR_KEYWORDS: &[&str] = &[
        "additionalItems",
        "additionalProperties",
        "contains",
        "items",
        "maxItems",
        "maxProperties",
        "minItems",
        "minProperties",
        "patternProperties",
        "prefixItems",
        "properties",
        "propertyNames",
        "required",
        "uniqueItems",
    ];
    for key in NON_SCALAR_KEYWORDS {
        obj.remove(*key);
    }
}

fn remove_object_keywords(obj: &mut Map<String, Value>) {
    const OBJECT_KEYWORDS: &[&str] = &[
        "additionalProperties",
        "maxProperties",
        "minProperties",
        "patternProperties",
        "properties",
        "propertyNames",
        "required",
    ];
    for key in OBJECT_KEYWORDS {
        obj.remove(*key);
    }
}
