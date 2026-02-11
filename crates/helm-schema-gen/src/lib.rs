mod merge;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use helm_schema_ir::{Guard, ValueUse};
use helm_schema_k8s::{K8sSchemaProvider, type_schema};

use merge::{merge_schema_list, merge_two_schemas};

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Generates a JSON Schema for Helm `values.yaml` from IR and a K8s schema provider.
pub trait ValuesSchemaGenerator {
    fn generate(&self, uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> Value;
}

// ---------------------------------------------------------------------------
// Default implementation
// ---------------------------------------------------------------------------

/// Default values schema generator.
///
/// Collects all `.Values.*` uses, infers their types from the K8s schema
/// provider and heuristics, merges conflicting schemas, and builds a nested
/// JSON Schema tree.
pub struct DefaultValuesSchemaGenerator;

impl ValuesSchemaGenerator for DefaultValuesSchemaGenerator {
    fn generate(&self, uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> Value {
        generate_values_schema(uses, provider)
    }
}

// ---------------------------------------------------------------------------
// Core generation logic
// ---------------------------------------------------------------------------

pub fn generate_values_schema(uses: &[ValueUse], provider: &dyn K8sSchemaProvider) -> Value {
    generate_values_schema_with_values_yaml(uses, provider, None)
}

pub fn generate_values_schema_with_values_yaml(
    uses: &[ValueUse],
    provider: &dyn K8sSchemaProvider,
    values_yaml: Option<&str>,
) -> Value {
    let mut referenced_value_paths: BTreeSet<String> = BTreeSet::new();
    let mut provider_schemas_by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut guard_boolish_by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut guard_constraints_by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for u in uses {
        if u.source_expr.trim().is_empty() {
            continue;
        }

        referenced_value_paths.insert(u.source_expr.clone());
        for g in &u.guards {
            for path in g.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                referenced_value_paths.insert(path.to_string());

                if let Some(schema) = infer_guard_boolish_schema(g) {
                    guard_boolish_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(schema);
                }
                if let Some(schema) = infer_guard_constraint_schema(g) {
                    guard_constraints_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(schema);
                }
            }
        }

        if !u.path.0.is_empty() {
            if let Some(schema) = provider.schema_for_use(u) {
                provider_schemas_by_value_path
                    .entry(u.source_expr.clone())
                    .or_default()
                    .push(schema);
            }
        }
    }

    let values_yaml_doc = values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);

    let mut root_schema = object_schema(Map::new());
    for vp in referenced_value_paths {
        let provider_schema = provider_schemas_by_value_path
            .remove(&vp)
            .map(merge_schema_list)
            .unwrap_or_else(empty_schema);

        let values_yaml_schema =
            lookup_values_yaml_schema(&values_yaml_doc, &vp).unwrap_or_else(empty_schema);

        let guard_boolish_schema = guard_boolish_by_value_path
            .remove(&vp)
            .map(merge_schema_list)
            .unwrap_or_else(empty_schema);

        let guard_constraint_schema = guard_constraints_by_value_path
            .remove(&vp)
            .map(merge_schema_list)
            .unwrap_or_else(empty_schema);

        let base = if !is_empty_schema(&provider_schema) {
            provider_schema
        } else if !is_empty_schema(&values_yaml_schema) {
            values_yaml_schema
        } else if !is_empty_schema(&guard_boolish_schema) {
            guard_boolish_schema
        } else {
            empty_schema()
        };

        let merged = if is_empty_schema(&guard_constraint_schema) {
            base
        } else if is_empty_schema(&base) {
            guard_constraint_schema
        } else {
            merge_two_schemas(base, guard_constraint_schema)
        };

        insert_schema_at_value_path(&mut root_schema, &vp, merged, false);
    }

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

// ---------------------------------------------------------------------------
// Inference helpers
// ---------------------------------------------------------------------------

fn is_empty_schema(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.is_empty())
}

fn empty_schema() -> Value {
    Value::Object(Map::new())
}

fn infer_guard_boolish_schema(guard: &Guard) -> Option<Value> {
    match guard {
        Guard::Eq { .. } => None,
        _ => Some(type_schema("boolean")),
    }
}

fn infer_guard_constraint_schema(guard: &Guard) -> Option<Value> {
    let Guard::Eq { value, .. } = guard else {
        return None;
    };
    Some(Value::Object(
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
    ))
}

fn lookup_values_yaml_schema(doc: &YamlValue, vp: &str) -> Option<Value> {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    let Some(v) = lookup_values_yaml_value(doc, &parts) else {
        return None;
    };
    Some(schema_from_yaml_value(v))
}

fn lookup_values_yaml_value<'a>(doc: &'a YamlValue, parts: &[&str]) -> Option<&'a YamlValue> {
    let mut cur = doc;
    for p in parts {
        match cur {
            YamlValue::Mapping(m) => {
                let k = YamlValue::String((*p).to_string());
                cur = m.get(&k)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

fn schema_from_yaml_value(v: &YamlValue) -> Value {
    match v {
        YamlValue::Null => empty_schema(),
        YamlValue::Bool(_) => type_schema("boolean"),
        YamlValue::Number(n) => {
            if n.as_i64().is_some() || n.as_u64().is_some() {
                type_schema("integer")
            } else {
                type_schema("number")
            }
        }
        YamlValue::String(_) => type_schema("string"),
        YamlValue::Sequence(seq) => {
            let items = if seq.is_empty() {
                empty_schema()
            } else {
                merge_schema_list(seq.iter().map(schema_from_yaml_value).collect())
            };
            Value::Object(
                [
                    ("type".to_string(), Value::String("array".to_string())),
                    ("items".to_string(), items),
                ]
                .into_iter()
                .collect(),
            )
        }
        YamlValue::Mapping(m) => {
            if m.is_empty() {
                return unknown_object_schema();
            }
            let mut props = Map::new();
            for (k, v) in m {
                let Some(key) = k.as_str() else {
                    continue;
                };
                props.insert(key.to_string(), schema_from_yaml_value(v));
            }
            object_schema(props)
        }
        _ => empty_schema(),
    }
}

// ---------------------------------------------------------------------------
// Schema tree construction
// ---------------------------------------------------------------------------

fn object_schema(properties: Map<String, Value>) -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            ("properties".to_string(), Value::Object(properties)),
            ("additionalProperties".to_string(), Value::Bool(false)),
        ]
        .into_iter()
        .collect(),
    )
}

fn unknown_object_schema() -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            (
                "additionalProperties".to_string(),
                Value::Object(Map::new()),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn insert_schema_at_value_path(root: &mut Value, vp: &str, leaf: Value, required: bool) {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return;
    }
    insert_schema_at_parts(root, &parts, leaf, required);
}

fn ensure_object_schema(v: &mut Value) {
    match v {
        Value::Object(obj) => {
            obj.insert("type".to_string(), Value::String("object".to_string()));
            obj.entry("properties".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if !obj.get("properties").and_then(|p| p.as_object()).is_some() {
                obj.insert("properties".to_string(), Value::Object(Map::new()));
            }
            obj.entry("additionalProperties".to_string())
                .or_insert(Value::Bool(false));

            let has_structure = obj
                .get("properties")
                .and_then(|v| v.as_object())
                .is_some_and(|m| !m.is_empty())
                || obj
                    .get("patternProperties")
                    .and_then(|v| v.as_object())
                    .is_some_and(|m| !m.is_empty())
                || obj
                    .get("required")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| !a.is_empty());

            let ap_is_empty_schema = obj
                .get("additionalProperties")
                .and_then(|v| v.as_object())
                .is_some_and(|m| m.is_empty());

            if has_structure && ap_is_empty_schema {
                obj.insert("additionalProperties".to_string(), Value::Bool(false));
            }
        }
        _ => {
            *v = object_schema(Map::new());
        }
    }
}

fn ensure_array_schema(v: &mut Value) {
    match v {
        Value::Object(obj) => {
            obj.insert("type".to_string(), Value::String("array".to_string()));
            obj.entry("items".to_string()).or_insert(Value::Null);
        }
        _ => {
            *v = Value::Object(
                [
                    ("type".to_string(), Value::String("array".to_string())),
                    ("items".to_string(), Value::Null),
                ]
                .into_iter()
                .collect(),
            );
        }
    }
}

fn ensure_items_schema(array_schema: &mut Value) -> &mut Value {
    array_schema
        .as_object_mut()
        .and_then(|o| o.get_mut("items"))
        .expect("array schema must have items")
}

fn ensure_required_contains(node: &mut Value, key: &str) {
    ensure_object_schema(node);
    let obj = node.as_object_mut().expect("object schema");
    let req = obj
        .entry("required")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(arr) = req.as_array_mut() else {
        *req = Value::Array(Vec::new());
        return ensure_required_contains(node, key);
    };
    if !arr.iter().any(|v| v.as_str() == Some(key)) {
        arr.push(Value::String(key.to_string()));
    }
    arr.sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
    arr.dedup();
}

const MAP_WILDCARD_SEGMENT: &str = "__any__";

fn insert_schema_at_parts(node: &mut Value, parts: &[&str], leaf: Value, required: bool) {
    if parts.is_empty() {
        return;
    }

    if parts[0] == MAP_WILDCARD_SEGMENT {
        ensure_object_schema(node);
        let obj = node.as_object_mut().expect("object schema");
        let ap = obj
            .entry("additionalProperties")
            .or_insert_with(|| Value::Object(Map::new()));
        if ap.as_bool() == Some(false) {
            *ap = Value::Object(Map::new());
        }
        if parts.len() == 1 {
            let existing = std::mem::replace(ap, Value::Null);
            *ap = match existing {
                Value::Null => leaf,
                other => merge_two_schemas(other, leaf),
            };
        } else {
            insert_schema_at_parts(ap, &parts[1..], leaf, false);
        }
        return;
    }

    if parts[0] == "*" {
        ensure_array_schema(node);
        let items = ensure_items_schema(node);
        if parts.len() == 1 {
            let existing = std::mem::replace(items, Value::Null);
            *items = match existing {
                Value::Null => leaf,
                other => merge_two_schemas(other, leaf),
            };
        } else {
            insert_schema_at_parts(items, &parts[1..], leaf, required);
        }
        return;
    }

    ensure_object_schema(node);
    if required {
        ensure_required_contains(node, parts[0]);
    }
    let props = node
        .as_object_mut()
        .and_then(|o| o.get_mut("properties"))
        .and_then(|v| v.as_object_mut())
        .expect("object schema must have properties");

    if parts.len() == 1 {
        let key = parts[0].to_string();
        match props.remove(&key) {
            None => {
                props.insert(key, leaf);
            }
            Some(existing) => {
                props.insert(key, merge_two_schemas(existing, leaf));
            }
        }
        return;
    }

    let key = parts[0].to_string();
    let child = props
        .entry(key)
        .or_insert_with(|| object_schema(Map::new()));
    insert_schema_at_parts(child, &parts[1..], leaf, required);
}

#[cfg(test)]
mod tests;
