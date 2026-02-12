use serde_json::{Map, Value};

pub fn merge_schema_list(mut schemas: Vec<Value>) -> Value {
    schemas.sort_by_key(canonical_json_string);
    schemas.dedup();
    let mut it = schemas.into_iter();
    let Some(first) = it.next() else {
        return Value::Object(Map::new());
    };
    it.fold(first, merge_two_schemas)
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

    if let Some(merged) = try_merge_compatible(&a, &b) {
        return merged;
    }

    let mut out: Vec<Value> = Vec::new();
    out.extend(flatten_anyof(a));
    out.extend(flatten_anyof(b));
    out.sort_by_key(canonical_json_string);
    out.dedup();
    if out.len() == 1 {
        out.into_iter().next().expect("len == 1")
    } else {
        Value::Object(
            [("anyOf".to_string(), Value::Array(out))]
                .into_iter()
                .collect(),
        )
    }
}

fn flatten_anyof(v: Value) -> Vec<Value> {
    if let Value::Object(obj) = &v
        && let Some(arr) = obj.get("anyOf").and_then(|x| x.as_array())
    {
        return arr.clone();
    }
    vec![v]
}

fn canonical_json_string(v: &Value) -> String {
    let v = canonicalize_json_value(v);
    serde_json::to_string(&v).unwrap_or_default()
}

fn canonicalize_json_value(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                if let Some(v) = o.get(k) {
                    out.insert(k.clone(), canonicalize_json_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(canonicalize_json_value).collect()),
        _ => v.clone(),
    }
}

fn schema_type(v: &Value) -> Option<&str> {
    v.as_object()?.get("type").and_then(|t| t.as_str())
}

fn try_merge_compatible(a: &Value, b: &Value) -> Option<Value> {
    let ta = schema_type(a)?;
    let tb = schema_type(b)?;
    if ta != tb {
        return None;
    }

    match ta {
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
            _ => {
                return None;
            }
        }
    }

    Some(Value::Object(out))
}

fn merge_object_schemas(a: &Value, b: &Value) -> Option<Value> {
    let mut out = a.as_object()?.clone();
    let bobj = b.as_object()?;

    fn is_structured_object(obj: &Map<String, Value>) -> bool {
        obj.get("properties")
            .and_then(|v| v.as_object())
            .is_some_and(|m| !m.is_empty())
            || obj
                .get("patternProperties")
                .and_then(|v| v.as_object())
                .is_some_and(|m| !m.is_empty())
            || obj
                .get("additionalProperties")
                .is_some_and(serde_json::Value::is_object)
            || obj
                .get("required")
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

    fn has_nonempty_props(obj: &Map<String, Value>) -> bool {
        obj.get("properties")
            .and_then(|v| v.as_object())
            .is_some_and(|m| !m.is_empty())
    }

    fn has_nonempty_pattern_props(obj: &Map<String, Value>) -> bool {
        obj.get("patternProperties")
            .and_then(|v| v.as_object())
            .is_some_and(|m| !m.is_empty())
    }

    fn has_required(obj: &Map<String, Value>) -> bool {
        obj.get("required")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty())
    }

    let a_fixed =
        has_nonempty_props(&out) || has_nonempty_pattern_props(&out) || has_required(&out);
    let b_fixed =
        has_nonempty_props(bobj) || has_nonempty_pattern_props(bobj) || has_required(bobj);

    let a_map_like = !a_fixed
        && out
            .get("additionalProperties")
            .is_some_and(serde_json::Value::is_object);
    let b_map_like = !b_fixed
        && bobj
            .get("additionalProperties")
            .is_some_and(serde_json::Value::is_object);

    match (
        out.get("additionalProperties").cloned(),
        bobj.get("additionalProperties").cloned(),
    ) {
        _ if a_fixed || b_fixed => {
            out.insert("additionalProperties".to_string(), Value::Bool(false));
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
    out.insert("type".to_string(), Value::String("object".to_string()));

    Some(Value::Object(out))
}
