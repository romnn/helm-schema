mod merge;

use std::collections::{BTreeMap, HashSet};

use serde_json::{Map, Value};

use helm_schema_ir::{Guard, ValueKind, ValueUse};
use helm_schema_k8s::{
    K8sSchemaProvider, path_pattern, strengthen_leaf_schema, string_map_schema, type_schema,
};

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
    let mut by_value_path: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut required_value_paths: HashSet<String> = HashSet::new();
    let mut guard_value_paths: HashSet<String> = HashSet::new();

    for u in uses {
        for g in &u.guards {
            for path in g.value_paths() {
                if !path.trim().is_empty() {
                    guard_value_paths.insert(path.to_string());
                }
            }
        }
    }

    for u in uses {
        if u.source_expr.trim().is_empty() {
            continue;
        }

        // Required inference: values used without any active guards are assumed
        // required, except for guard-like booleans and paths observed as guards.
        if u.guards.is_empty()
            && !u.path.0.is_empty()
            && !guard_value_paths.contains(&u.source_expr)
            && infer_guard_schema(&u.source_expr, None)
                .as_object()
                .is_some_and(|o| o.is_empty())
        {
            required_value_paths.insert(u.source_expr.clone());
        }

        for g in &u.guards {
            for path in g.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                let gs = infer_guard_schema(path, Some(g));
                if gs.as_object().is_some_and(|o| o.is_empty()) {
                    continue;
                }
                by_value_path.entry(path.to_string()).or_default().push(gs);
            }
        }

        let inferred = match u.kind {
            ValueKind::Scalar => provider
                .schema_for_use(u)
                .or_else(|| infer_fallback_schema(u)),
            ValueKind::Fragment => provider.schema_for_use(u).or_else(|| {
                if u.source_expr.ends_with("annotations") || u.source_expr.ends_with("labels") {
                    Some(string_map_schema())
                } else {
                    Some(unknown_object_schema())
                }
            }),
        };

        let Some(schema) = inferred else {
            continue;
        };

        by_value_path
            .entry(u.source_expr.clone())
            .or_default()
            .push(schema);
    }

    let mut root_schema = object_schema(Map::new());
    for (vp, schemas) in by_value_path {
        let merged = merge_schema_list(schemas);
        let merged = strengthen_leaf_schema(&vp, merged);
        let is_required = required_value_paths.contains(&vp);
        insert_schema_at_value_path(&mut root_schema, &vp, merged, is_required);
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

fn infer_guard_schema(guard_expr: &str, guard: Option<&Guard>) -> Value {
    // Eq guards produce enum schemas.
    if let Some(Guard::Eq { value, .. }) = guard {
        return Value::Object(
            [(
                "enum".to_string(),
                Value::Array(vec![Value::String(value.clone())]),
            )]
            .into_iter()
            .collect(),
        );
    }

    if guard_expr == "installCRDs"
        || guard_expr.ends_with(".enabled")
        || guard_expr.ends_with("Enabled")
    {
        return type_schema("boolean");
    }
    Value::Object(Map::new())
}

fn infer_fallback_schema(u: &ValueUse) -> Option<Value> {
    if u.source_expr == "installCRDs"
        || u.source_expr.ends_with(".enabled")
        || u.source_expr.ends_with("Enabled")
    {
        return Some(type_schema("boolean"));
    }

    let pat = path_pattern(&u.path);
    match pat.as_str() {
        "metadata.annotations" | "metadata.labels" => Some(string_map_schema()),
        "spec.replicas" => Some(type_schema("integer")),
        _ => {
            let last = u.path.0.last().map(|s| s.as_str()).unwrap_or("");
            if matches!(
                last,
                "replicas"
                    | "replicaCount"
                    | "revisionHistoryLimit"
                    | "terminationGracePeriodSeconds"
                    | "port"
                    | "targetPort"
                    | "nodePort"
                    | "containerPort"
                    | "hostPort"
                    | "number"
            ) {
                return Some(type_schema("integer"));
            }

            if last == "image" {
                return Some(type_schema("string"));
            }

            if u.source_expr.ends_with("annotations") || u.source_expr.ends_with("labels") {
                return Some(string_map_schema());
            }

            Some(type_schema("string"))
        }
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
