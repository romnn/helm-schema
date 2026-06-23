use serde_json::{Map, Value};

use crate::schema_model::schema_type;
use crate::schema_node::SchemaNode;

fn is_annotation_keyword(key: &str) -> bool {
    matches!(
        key,
        "description" | "title" | "default" | "examples" | "deprecated" | "readOnly" | "writeOnly"
    )
}

pub fn merge_schema_list(mut schemas: Vec<Value>) -> Value {
    match schemas.len() {
        0 => return empty_schema(),
        1 => return schemas.pop().unwrap_or_else(empty_schema),
        2 => {
            let right = schemas.pop().unwrap_or_else(empty_schema);
            let left = schemas.pop().unwrap_or_else(empty_schema);
            return merge_two_schemas(left, right);
        }
        _ => {}
    }

    schemas = dedup_schemas(schemas);
    let mut it = schemas.into_iter();
    let Some(first) = it.next() else {
        return empty_schema();
    };
    it.fold(first, merge_two_schemas)
}

pub fn union_schema_list(mut schemas: Vec<Value>) -> Value {
    match schemas.len() {
        0 => return empty_schema(),
        1 => return schemas.pop().unwrap_or_else(empty_schema),
        _ => {}
    }

    let mut out: Vec<Value> = Vec::new();
    for schema in schemas {
        out.extend(flatten_union_variants(schema));
    }
    if out
        .iter()
        .any(|schema| !schema.as_object().is_some_and(Map::is_empty))
    {
        out.retain(|schema| !schema.as_object().is_some_and(Map::is_empty));
    }
    out = dedup_schemas(out);
    out.sort_by_key(canonical_json_string);
    if out.len() == 1 {
        out.into_iter().next().expect("len == 1")
    } else {
        any_of_schema(out)
    }
}

pub fn merge_two_schemas(a: Value, b: Value) -> Value {
    if a == b {
        return a;
    }

    if a.as_object().is_some_and(serde_json::Map::is_empty) {
        return b;
    }
    if b.as_object().is_some_and(serde_json::Map::is_empty) {
        return a;
    }

    if union_contains_schema(&a, &b) {
        return a;
    }
    if union_contains_schema(&b, &a) {
        return b;
    }

    if let Some(merged) = try_merge_nullable_scalar_schema(&a, &b) {
        return merged;
    }

    if let Some(merged) = try_merge_compatible(&a, &b) {
        return merged;
    }

    let mut out: Vec<Value> = Vec::new();
    out.extend(flatten_union_variants(a));
    out.extend(flatten_union_variants(b));
    out = collapse_compatible_variants(out);
    out = dedup_schemas(out);
    out.sort_by_key(canonical_json_string);
    if out.len() == 1 {
        out.into_iter().next().expect("len == 1")
    } else {
        any_of_schema(out)
    }
}

fn empty_schema() -> Value {
    SchemaNode::empty().into_value()
}

fn any_of_schema(schemas: Vec<Value>) -> Value {
    SchemaNode::any_of(schemas.into_iter().map(SchemaNode::foreign).collect()).into_value()
}

fn flatten_union_variants(v: Value) -> Vec<Value> {
    if let Value::Object(obj) = &v
        && let Some(arr) = obj.get("anyOf").and_then(|x| x.as_array())
    {
        return arr.clone();
    }
    if let Value::Object(mut obj) = v.clone()
        && let Some(Value::Array(types)) = obj.remove("type")
    {
        let mut variants = Vec::new();
        for ty in types {
            let Some(ty) = ty.as_str() else {
                continue;
            };
            let mut variant = obj.clone();
            if ty == "null" {
                variant.retain(|key, _| key == "type");
            }
            variant.insert("type".to_string(), Value::String(ty.to_string()));
            variants.push(Value::Object(variant));
        }
        if !variants.is_empty() {
            return variants;
        }
    }
    vec![v]
}

fn union_contains_schema(union: &Value, candidate: &Value) -> bool {
    union_variants(union).is_some_and(|variants| {
        variants
            .iter()
            .any(|variant| variant == candidate || union_contains_schema(variant, candidate))
    })
}

fn union_variants(schema: &Value) -> Option<&Vec<Value>> {
    let object = schema.as_object()?;
    object
        .get("anyOf")
        .and_then(Value::as_array)
        .or_else(|| object.get("oneOf").and_then(Value::as_array))
}

fn try_merge_nullable_scalar_schema(a: &Value, b: &Value) -> Option<Value> {
    match (schema_type(a), schema_type(b)) {
        (Some("null"), Some(scalar_type)) => nullable_scalar_schema(b, scalar_type),
        (Some(scalar_type), Some("null")) => nullable_scalar_schema(a, scalar_type),
        _ => None,
    }
}

fn nullable_scalar_schema(schema: &Value, scalar_type: &str) -> Option<Value> {
    if !matches!(scalar_type, "boolean" | "integer" | "number" | "string") {
        return None;
    }

    let mut object = schema.as_object()?.clone();
    if object.contains_key("enum") || object.contains_key("const") {
        return None;
    }
    object.insert(
        "type".to_string(),
        Value::Array(vec![
            Value::String(scalar_type.to_string()),
            Value::String("null".to_string()),
        ]),
    );
    Some(Value::Object(object))
}

fn collapse_compatible_variants(variants: Vec<Value>) -> Vec<Value> {
    if variants.len() < 2 {
        return variants;
    }

    let mut out: Vec<Value> = Vec::new();
    'variants: for variant in variants {
        for existing in &mut out {
            if let Some(merged) = try_merge_compatible(existing, &variant) {
                *existing = merged;
                continue 'variants;
            }
        }
        out.push(variant);
    }
    out
}

fn dedup_schemas(schemas: Vec<Value>) -> Vec<Value> {
    if schemas.len() < 2 {
        return schemas;
    }

    let mut out = Vec::new();
    for schema in schemas {
        if out.iter().any(|existing| existing == &schema) {
            continue;
        }
        out.push(schema);
    }
    out
}

fn canonical_json_string(v: &Value) -> String {
    let v = canonicalize_json_value(v);
    serde_json::to_string(&v).expect("serialize canonical json schema value")
}

fn canonicalize_json_value(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                if let Some(value) = o.get(key) {
                    out.insert(key.clone(), canonicalize_json_value(value));
                }
            }
            Value::Object(out)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json_value).collect()),
        _ => v.clone(),
    }
}

fn is_exact_empty_object_schema(v: &Value) -> bool {
    let Some(obj) = v.as_object() else {
        return false;
    };
    schema_type(v) == Some("object") && obj.get("maxProperties").and_then(Value::as_u64) == Some(0)
}

fn try_merge_compatible(a: &Value, b: &Value) -> Option<Value> {
    let ta = schema_type(a)?;
    let tb = schema_type(b)?;
    if ta != tb {
        return None;
    }

    match ta {
        "object" if is_exact_empty_object_schema(a) || is_exact_empty_object_schema(b) => None,
        "object" => merge_object_schemas(a, b),
        "array" => merge_array_schemas(a, b),
        _ => merge_scalar_like_schemas(a, b),
    }
}

fn merge_array_schemas(a: &Value, b: &Value) -> Option<Value> {
    let mut out = a.as_object()?.clone();
    let bobj = b.as_object()?;

    match (out.get("items").cloned(), bobj.get("items").cloned()) {
        (Some(items_a), Some(items_b)) => {
            if !items_a.is_null() && !items_b.is_null() {
                out.insert("items".to_string(), merge_two_schemas(items_a, items_b));
            } else if items_a.is_null() {
                out.insert("items".to_string(), items_b);
            }
        }
        (None, Some(items_b)) => {
            out.insert("items".to_string(), items_b);
        }
        _ => {}
    }

    let out_items_is_null = out.get("items").is_some_and(serde_json::Value::is_null);
    let b_items_is_null = bobj.get("items").is_some_and(serde_json::Value::is_null);
    if out_items_is_null && !b_items_is_null {
        out.insert(
            "items".to_string(),
            bobj.get("items").cloned().unwrap_or(Value::Null),
        );
    }

    for (k, bv) in bobj {
        if k == "type" || k == "items" {
            continue;
        }
        match out.get(k) {
            None => {
                out.insert(k.clone(), bv.clone());
            }
            Some(av) if av == bv => {}
            _ => {
                return None;
            }
        }
    }

    out.insert("type".to_string(), Value::String("array".to_string()));
    out.entry("items".to_string()).or_insert(Value::Null);
    Some(Value::Object(out))
}

fn merge_scalar_like_schemas(a: &Value, b: &Value) -> Option<Value> {
    let mut out = a.as_object()?.clone();
    let bobj = b.as_object()?;
    let is_string_type = out.get("type").and_then(Value::as_str) == Some("string");

    match (
        out.get("enum").and_then(|v| v.as_array()).cloned(),
        bobj.get("enum").and_then(|v| v.as_array()).cloned(),
    ) {
        (Some(ae), Some(be)) => {
            let mut inter: Vec<Value> = ae.into_iter().filter(|v| be.contains(v)).collect();
            inter.sort_by_key(std::string::ToString::to_string);
            inter.dedup();
            if inter.is_empty() {
                return None;
            }
            out.insert("enum".to_string(), Value::Array(inter));
        }
        (None, Some(be)) => {
            out.insert("enum".to_string(), Value::Array(be));
        }
        _ => {}
    }

    for (k, bv) in bobj {
        if k == "type" || k == "enum" {
            continue;
        }
        match out.get(k) {
            None => {
                out.insert(k.clone(), bv.clone());
            }
            Some(av) if av == bv => {}
            Some(_) if is_string_type => {
                out.remove(k);
            }
            Some(_) if is_annotation_keyword(k) => {
                out.remove(k);
            }
            _ => {
                return None;
            }
        }
    }

    if let Some(values) = out.get("enum").and_then(Value::as_array)
        && !values
            .iter()
            .all(|value| enum_value_satisfies_scalar_schema(value, &out))
    {
        return None;
    }

    Some(Value::Object(out))
}

fn enum_value_satisfies_scalar_schema(value: &Value, schema: &Map<String, Value>) -> bool {
    match schema.get("type").and_then(Value::as_str) {
        Some("string") => {
            let Some(value) = value.as_str() else {
                return false;
            };
            let len = value.chars().count() as u64;
            if schema
                .get("minLength")
                .and_then(Value::as_u64)
                .is_some_and(|min_length| len < min_length)
            {
                return false;
            }
            if schema
                .get("maxLength")
                .and_then(Value::as_u64)
                .is_some_and(|max_length| len > max_length)
            {
                return false;
            }
            !schema.contains_key("pattern")
        }
        Some("integer") => value.as_i64().is_some() || value.as_u64().is_some(),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        Some("null") => value.is_null(),
        _ => true,
    }
}

#[allow(
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::match_same_arms
)]
fn merge_object_schemas(a: &Value, b: &Value) -> Option<Value> {
    let mut out = a.as_object()?.clone();
    let bobj = b.as_object()?;

    fn has_meaningful_additional_properties(obj: &Map<String, Value>) -> bool {
        obj.get("additionalProperties")
            .and_then(|value| value.as_object())
            .is_some_and(|map| !map.is_empty())
            || preserves_unknown_fields(obj)
    }

    fn preserves_unknown_fields(obj: &Map<String, Value>) -> bool {
        obj.get("x-kubernetes-preserve-unknown-fields")
            .and_then(Value::as_bool)
            == Some(true)
    }

    fn is_structured_object(obj: &Map<String, Value>) -> bool {
        obj.get("properties")
            .and_then(|v| v.as_object())
            .is_some_and(|m| !m.is_empty())
            || obj
                .get("patternProperties")
                .and_then(|v| v.as_object())
                .is_some_and(|m| !m.is_empty())
            || has_meaningful_additional_properties(obj)
            || obj
                .get("required")
                .and_then(|v| v.as_array())
                .is_some_and(|a| !a.is_empty())
            || obj
                .get("allOf")
                .and_then(|v| v.as_array())
                .is_some_and(|a| !a.is_empty())
    }

    let a_structured = is_structured_object(&out);
    let b_structured = is_structured_object(bobj);
    if !a_structured && b_structured {
        return Some(Value::Object(bobj.clone()));
    }
    if !b_structured && a_structured {
        return Some(Value::Object(out));
    }

    let a_map_like = has_meaningful_additional_properties(&out);
    let b_map_like = has_meaningful_additional_properties(bobj);
    let a_preserves_unknown = preserves_unknown_fields(&out);
    let b_preserves_unknown = preserves_unknown_fields(bobj);

    match (
        out.get("additionalProperties").cloned(),
        bobj.get("additionalProperties").cloned(),
    ) {
        _ if a_preserves_unknown || b_preserves_unknown => {
            out.remove("additionalProperties");
        }
        (Some(ap_a), Some(ap_b)) if a_map_like && b_map_like => {
            out.insert(
                "additionalProperties".to_string(),
                merge_two_schemas(ap_a, ap_b),
            );
        }
        (Some(Value::Bool(false)), Some(ap_b)) if ap_b.is_object() => {
            out.insert("additionalProperties".to_string(), ap_b);
        }
        (Some(ap_a), Some(Value::Bool(false))) if ap_a.is_object() => {
            out.insert("additionalProperties".to_string(), ap_a);
        }
        (Some(Value::Bool(false)), _) | (_, Some(Value::Bool(false))) => {
            out.insert("additionalProperties".to_string(), Value::Bool(false));
        }
        (Some(Value::Bool(true)), Some(ap_b)) => {
            out.insert("additionalProperties".to_string(), ap_b);
        }
        (Some(ap_a), Some(Value::Bool(true))) => {
            out.insert("additionalProperties".to_string(), ap_a);
        }
        (Some(ap_a), Some(ap_b)) => {
            out.insert(
                "additionalProperties".to_string(),
                merge_two_schemas(ap_a, ap_b),
            );
        }
        (None, Some(ap_b)) => {
            out.insert("additionalProperties".to_string(), ap_b);
        }
        _ => {}
    }

    // Merge required lists by union.
    let mut required: Vec<String> = out
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(std::string::ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(breq) = bobj.get("required").and_then(|v| v.as_array()) {
        for v in breq {
            if let Some(s) = v.as_str() {
                required.push(s.to_string());
            }
        }
    }
    required.sort();
    required.dedup();
    if !required.is_empty() {
        out.insert(
            "required".to_string(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }

    // Merge properties recursively.
    let mut props = out
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_else(Map::new);
    let bprops = bobj
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_else(Map::new);
    for (k, bv) in bprops {
        match props.remove(&k) {
            None => {
                props.insert(k, bv);
            }
            Some(av) => {
                props.insert(k, merge_two_schemas(av, bv));
            }
        }
    }
    out.insert("properties".to_string(), Value::Object(props));

    // Merge patternProperties recursively.
    let mut pp = out
        .get("patternProperties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_else(Map::new);
    let bpp = bobj
        .get("patternProperties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_else(Map::new);
    for (k, bv) in bpp {
        match pp.remove(&k) {
            None => {
                pp.insert(k, bv);
            }
            Some(av) => {
                pp.insert(k, merge_two_schemas(av, bv));
            }
        }
    }
    if !pp.is_empty() {
        out.insert("patternProperties".to_string(), Value::Object(pp));
    }

    let mut all_of = out
        .get("allOf")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(b_all_of) = bobj.get("allOf").and_then(Value::as_array) {
        all_of.extend(b_all_of.iter().cloned());
    }
    all_of = dedup_schemas(all_of);
    if !all_of.is_empty() {
        out.insert("allOf".to_string(), Value::Array(all_of));
    }

    out.insert("type".to_string(), Value::String("object".to_string()));

    Some(Value::Object(out))
}

#[cfg(test)]
#[path = "tests/merge.rs"]
mod tests;
