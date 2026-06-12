use serde_json::{Map, Value};

use helm_schema_ir::{Guard, ValueKind, ValueUse};
use helm_schema_k8s::type_schema;

/// Generator-side policy for lowering semantic value uses into schema evidence.
///
/// The IR still crosses into the generator as [`ValueUse`] DTOs, but the
/// decisions about provider-schema domains and guard-derived constraints live
/// here rather than being spread across root-schema construction.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ResolvePolicy;

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
