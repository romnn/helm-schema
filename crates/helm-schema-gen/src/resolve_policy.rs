use serde_json::{Map, Value};

use helm_schema_ir::{GuardConstraint, ProviderSchemaUse, ValueKind};
use helm_schema_k8s::type_schema;

use crate::merge::{merge_schema_list, merge_two_schemas, union_schema_list};
use crate::path_schema::{
    EmptyMapPlaceholderUse, empty_map_placeholder_has_structural_object_use,
    generalize_fixed_object_schema_to_open_map, merge_explicit_empty_placeholder,
    open_fragment_values_schema,
};
use crate::schema_model::{
    add_null_schema, empty_schema, empty_string_schema, is_empty_schema, is_fixed_object_schema,
    is_object_or_array_schema, is_open_string_map_schema, is_scalar_like_schema, is_scalar_schema,
    schema_allows_scalar_type, schema_permits_empty_string, schema_type,
};
use crate::schema_tree::unknown_object_schema;

/// Generator-side policy for lowering semantic value uses into schema evidence.
///
/// Decisions about provider-schema domains and guard-derived constraints live
/// here rather than being spread across root-schema construction.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ResolvePolicy;

/// Structural facts for one `.Values.*` path.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ValuePathSchemaFacts {
    pub(crate) has_referenced_descendants: bool,
    pub(crate) used_as_fragment: bool,
    pub(crate) is_ranged_source: bool,
    pub(crate) is_partial_scalar_value_path: bool,
    pub(crate) path_has_render_use: bool,
    pub(crate) path_all_render_uses_self_guarded: bool,
    pub(crate) path_has_self_range_guard_render_use: bool,
    pub(crate) contract_path_is_nullable: bool,
    pub(crate) has_type_hint: bool,
    pub(crate) values_yaml_has_no_schema_evidence: bool,
    pub(crate) values_yaml_is_explicit_null: bool,
    pub(crate) values_yaml_is_empty_string: bool,
    pub(crate) values_yaml_is_empty_map: bool,
    pub(crate) values_yaml_is_mapping: bool,
}

impl ValuePathSchemaFacts {
    fn has_explicit_null_scalar_default(
        self,
        type_hint_schema: &Value,
        guard_constraint_schema: &Value,
    ) -> bool {
        self.values_yaml_is_explicit_null
            && (is_scalar_like_schema(type_hint_schema)
                || is_scalar_like_schema(guard_constraint_schema))
    }

    fn accepts_null_default(
        self,
        type_hint_schema: &Value,
        guard_constraint_schema: &Value,
    ) -> bool {
        self.contract_path_is_nullable
            || self.has_type_hint
            || self.has_explicit_null_scalar_default(type_hint_schema, guard_constraint_schema)
    }

    fn preserve_explicit_null_default(
        self,
        type_hint_schema: &Value,
        guard_constraint_schema: &Value,
    ) -> bool {
        self.values_yaml_is_explicit_null
            && self.accepts_null_default(type_hint_schema, guard_constraint_schema)
    }

    fn preserve_empty_string_fallback(
        self,
        type_hint_schema: &Value,
        guard_constraint_schema: &Value,
    ) -> bool {
        self.values_yaml_is_empty_string
            && ((self.path_has_render_use && self.path_all_render_uses_self_guarded)
                || is_scalar_like_schema(type_hint_schema)
                || is_scalar_like_schema(guard_constraint_schema))
    }

    fn empty_map_placeholder_use(self) -> EmptyMapPlaceholderUse {
        EmptyMapPlaceholderUse {
            is_empty_map: self.values_yaml_is_empty_map,
            is_ranged_source: self.is_ranged_source,
            has_self_range_guard_render_use: self.path_has_self_range_guard_render_use,
            has_render_use: self.path_has_render_use,
            all_render_uses_self_guarded: self.path_all_render_uses_self_guarded,
            used_as_fragment: self.used_as_fragment,
        }
    }
}

/// Inputs for one value-path schema decision.
///
/// These are the evidence streams collected for a single `.Values.*` path
/// before the policy decides which schemas to prefer, merge, or preserve.
pub(crate) struct ValuePathSchemaInputs {
    pub(crate) facts: ValuePathSchemaFacts,
    pub(crate) provider_schema: Value,
    pub(crate) values_yaml_schema: Value,
    pub(crate) guard_constraint_schema: Value,
    pub(crate) type_hint_schema: Value,
}

struct ValuePathMergeInputs {
    facts: ValuePathSchemaFacts,
    provider_schema: Value,
    values_yaml_schema: Value,
    guard_constraint_schema: Value,
    type_hint_schema: Value,
    preserve_empty_string_fallback: bool,
}

impl ResolvePolicy {
    pub(crate) fn provider_schema_for_value_use(
        &self,
        schema: &Value,
        use_: &ProviderSchemaUse,
    ) -> Option<Value> {
        match use_.kind {
            ValueKind::Fragment => Some(schema.clone()),
            ValueKind::PartialScalar => None,
            ValueKind::Scalar if use_.is_self_range_collection => {
                restrict_schema_to_scalar_collection_domain(schema.clone())
            }
            ValueKind::Scalar => restrict_schema_to_scalar_domain(schema.clone()),
        }
    }

    pub(crate) fn guard_constraint_schema(&self, constraint: &GuardConstraint) -> Option<Value> {
        match constraint {
            GuardConstraint::Eq { value } => Some(Value::Object(
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
            GuardConstraint::TypeIs { schema_type } => match schema_type.as_str() {
                "array" | "boolean" | "integer" | "number" | "object" | "string" => {
                    Some(type_schema(schema_type))
                }
                _ => None,
            },
        }
    }

    fn restrict_to_scalar_domain(&self, schema: Value) -> Option<Value> {
        restrict_schema_to_scalar_domain(schema)
    }

    pub(crate) fn resolve_schema_for_value_path(&self, input: ValuePathSchemaInputs) -> Value {
        let ValuePathSchemaInputs {
            facts,
            provider_schema,
            values_yaml_schema,
            guard_constraint_schema,
            type_hint_schema,
        } = input;
        let preserve_explicit_null_default =
            facts.preserve_explicit_null_default(&type_hint_schema, &guard_constraint_schema);
        let preserve_empty_string_fallback =
            facts.preserve_empty_string_fallback(&type_hint_schema, &guard_constraint_schema);
        let values_yaml_schema = self.adjust_values_yaml_schema_for_value_path(
            values_yaml_schema,
            facts,
            &provider_schema,
        );
        let provider_schema = self.adjust_provider_schema_for_value_path(
            facts,
            provider_schema,
            &values_yaml_schema,
            &type_hint_schema,
            &guard_constraint_schema,
        );
        let partial_scalar_schema = self.partial_scalar_schema_for_value_path(
            facts,
            &provider_schema,
            &type_hint_schema,
            &guard_constraint_schema,
        );
        let guard_constraint_schema =
            merge_schema_list(vec![guard_constraint_schema, partial_scalar_schema]);
        let merged = self.resolve_merged_schema_for_value_path(ValuePathMergeInputs {
            facts,
            provider_schema,
            values_yaml_schema,
            guard_constraint_schema,
            type_hint_schema,
            preserve_empty_string_fallback,
        });

        if (preserve_explicit_null_default
            || (is_scalar_like_schema(&merged) && facts.contract_path_is_nullable))
            && !is_empty_schema(&merged)
        {
            add_null_schema(merged)
        } else if preserve_explicit_null_default {
            type_schema("null")
        } else if empty_map_placeholder_has_structural_object_use(
            &merged,
            facts.empty_map_placeholder_use(),
        ) {
            merge_explicit_empty_placeholder(merged, facts.values_yaml_is_empty_map)
        } else {
            merged
        }
    }

    fn adjust_values_yaml_schema_for_value_path(
        &self,
        values_yaml_schema: Value,
        facts: ValuePathSchemaFacts,
        provider_schema: &Value,
    ) -> Value {
        let values_yaml_schema = if empty_map_placeholder_has_structural_object_use(
            provider_schema,
            facts.empty_map_placeholder_use(),
        ) {
            empty_schema()
        } else {
            values_yaml_schema
        };
        let values_yaml_schema = if facts.used_as_fragment && is_empty_schema(provider_schema) {
            open_fragment_values_schema(values_yaml_schema)
        } else {
            values_yaml_schema
        };

        if facts.is_ranged_source && facts.values_yaml_is_mapping {
            generalize_fixed_object_schema_to_open_map(values_yaml_schema)
        } else {
            values_yaml_schema
        }
    }

    fn adjust_provider_schema_for_value_path(
        &self,
        facts: ValuePathSchemaFacts,
        provider_schema: Value,
        values_yaml_schema: &Value,
        type_hint_schema: &Value,
        guard_constraint_schema: &Value,
    ) -> Value {
        if facts.used_as_fragment
            && is_scalar_schema(values_yaml_schema)
            && (is_scalar_like_schema(type_hint_schema)
                || is_scalar_like_schema(guard_constraint_schema))
        {
            self.restrict_to_scalar_domain(provider_schema.clone())
                .unwrap_or(provider_schema)
        } else {
            provider_schema
        }
    }

    fn partial_scalar_schema_for_value_path(
        &self,
        facts: ValuePathSchemaFacts,
        provider_schema: &Value,
        type_hint_schema: &Value,
        guard_constraint_schema: &Value,
    ) -> Value {
        if facts.is_partial_scalar_value_path
            && is_empty_schema(provider_schema)
            && is_empty_schema(type_hint_schema)
            && is_empty_schema(guard_constraint_schema)
            && facts.values_yaml_has_no_schema_evidence
        {
            type_schema("string")
        } else {
            empty_schema()
        }
    }

    fn resolve_merged_schema_for_value_path(&self, input: ValuePathMergeInputs) -> Value {
        let base = if !is_empty_schema(&input.provider_schema) {
            if is_empty_schema(&input.values_yaml_schema) {
                input.provider_schema
            } else {
                // Some charts use scalar "preset" values that are fed into helpers which
                // expand into full K8s objects in the rendered manifest (e.g. affinity presets).
                // In these cases the *input* type in values.yaml is the scalar, not the output
                // object type, so prefer the values.yaml scalar schema.
                if input.facts.has_referenced_descendants
                    && is_fixed_object_schema(&input.values_yaml_schema)
                    && is_scalar_schema(&input.provider_schema)
                {
                    input.values_yaml_schema
                } else if input.facts.used_as_fragment
                    && is_fixed_object_schema(&input.values_yaml_schema)
                    && is_open_string_map_schema(&input.provider_schema)
                {
                    input.provider_schema
                } else if input.facts.used_as_fragment
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
        } else if input.facts.used_as_fragment {
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
