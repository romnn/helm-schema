use serde_json::{Map, Value};

fn is_annotation_keyword(key: &str) -> bool {
    matches!(
        key,
        "description" | "title" | "default" | "examples" | "deprecated" | "readOnly" | "writeOnly"
    )
}

pub fn merge_schema_list(mut schemas: Vec<Value>) -> Value {
    match schemas.len() {
        0 => return Value::Object(Map::new()),
        1 => return schemas.pop().unwrap_or_else(|| Value::Object(Map::new())),
        2 => {
            let right = schemas.pop().unwrap_or_else(|| Value::Object(Map::new()));
            let left = schemas.pop().unwrap_or_else(|| Value::Object(Map::new()));
            return merge_two_schemas(left, right);
        }
        _ => {}
    }

    schemas = dedup_schemas(schemas);
    let mut it = schemas.into_iter();
    let Some(first) = it.next() else {
        return Value::Object(Map::new());
    };
    it.fold(first, merge_two_schemas)
}

pub fn union_schema_list(mut schemas: Vec<Value>) -> Value {
    match schemas.len() {
        0 => return Value::Object(Map::new()),
        1 => return schemas.pop().unwrap_or_else(|| Value::Object(Map::new())),
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
        Value::Object(
            [("anyOf".to_string(), Value::Array(out))]
                .into_iter()
                .collect(),
        )
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
        Value::Object(
            [("anyOf".to_string(), Value::Array(out))]
                .into_iter()
                .collect(),
        )
    }
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
    serde_json::to_string(&v).unwrap_or_default()
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

fn schema_type(v: &Value) -> Option<&str> {
    v.as_object()?.get("type").and_then(|t| t.as_str())
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

    Some(Value::Object(out))
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
    out.insert("type".to_string(), Value::String("object".to_string()));

    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::merge_two_schemas;
    use serde_json::Value;
    use serde_json::json;

    #[test]
    fn merge_open_string_map_with_fixed_values_object_keeps_map_open() {
        let open_map = json!({
            "type": "object",
            "additionalProperties": { "type": "string" }
        });
        let fixed_values_object = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "cert-manager.io/cluster-issuer": { "type": "string" }
            }
        });

        let merged = merge_two_schemas(open_map, fixed_values_object);

        assert_eq!(
            merged
                .pointer("/additionalProperties/type")
                .and_then(|value| value.as_str()),
            Some("string"),
        );
        assert_eq!(
            merged
                .pointer("/properties/cert-manager.io~1cluster-issuer/type")
                .and_then(|value| value.as_str()),
            Some("string"),
        );
    }

    #[test]
    fn merge_nested_open_quantity_map_with_fixed_values_object_keeps_map_open() {
        let provider = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "requests": {
                    "type": "object",
                    "description": "Requests describes the minimum amount of compute resources required.",
                    "additionalProperties": {
                        "oneOf": [
                            { "type": "string" },
                            { "type": "number" }
                        ]
                    }
                }
            }
        });
        let values_yaml = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "requests": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "cpu": { "type": "string" }
                    }
                }
            }
        });

        let merged = merge_two_schemas(provider, values_yaml);

        assert!(
            merged
                .pointer("/properties/requests/additionalProperties/oneOf")
                .and_then(|value| value.as_array())
                .is_some(),
            "expected nested requests map to stay open, got {merged}",
        );
        assert_eq!(
            merged
                .pointer("/properties/requests/properties/cpu/type")
                .and_then(|value| value.as_str()),
            Some("string"),
        );
    }

    #[test]
    fn merge_preserve_unknown_fields_object_with_closed_values_object_stays_open() {
        let provider = json!({
            "type": "object",
            "x-kubernetes-preserve-unknown-fields": true
        });
        let values_yaml = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "replicas": { "type": "integer" },
                "logLevel": { "type": "string" }
            }
        });

        let merged = merge_two_schemas(provider, values_yaml);

        assert_eq!(
            merged.get("additionalProperties"),
            None,
            "preserve-unknown-fields object should stay open, got {merged}"
        );
        assert_eq!(
            merged
                .pointer("/properties/replicas/type")
                .and_then(serde_json::Value::as_str),
            Some("integer"),
        );
        assert_eq!(
            merged
                .get("x-kubernetes-preserve-unknown-fields")
                .and_then(serde_json::Value::as_bool),
            Some(true),
        );
    }

    #[test]
    fn merge_open_values_object_with_exact_empty_union_preserves_empty_branch() {
        let values_placeholder = json!({
            "type": "object",
            "additionalProperties": {}
        });
        let exact_empty = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {},
            "maxProperties": 0
        });
        let provider = json!({
            "type": "object",
            "required": ["kind", "name"],
            "properties": {
                "kind": { "type": "string" },
                "name": { "type": "string" }
            }
        });

        let merged = merge_two_schemas(
            values_placeholder,
            json!({
                "anyOf": [
                    exact_empty,
                    provider
                ]
            }),
        );
        let variants = merged
            .get("anyOf")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("expected exact-empty/provider union, got {merged}"));

        assert!(
            variants
                .iter()
                .any(|variant| variant.get("maxProperties").and_then(Value::as_u64) == Some(0)),
            "exact empty object branch should survive merge, got {merged}",
        );
        assert!(
            variants.iter().any(|variant| {
                variant
                    .get("required")
                    .and_then(Value::as_array)
                    .is_some_and(|required| {
                        required.iter().any(|value| value.as_str() == Some("kind"))
                            && required.iter().any(|value| value.as_str() == Some("name"))
                    })
            }),
            "provider-required object branch should survive merge, got {merged}",
        );
    }

    #[test]
    fn merge_scalar_schemas_drops_conflicting_annotations() {
        let metadata_name = json!({
            "type": "string",
            "description": "Name must be unique within a namespace."
        });
        let service_account_name = json!({
            "type": "string",
            "description": "ServiceAccountName is the name of the ServiceAccount to use."
        });

        let merged = merge_two_schemas(metadata_name, service_account_name);

        assert_eq!(merged, json!({ "type": "string" }));
    }

    #[test]
    fn merge_string_schemas_drops_conflicting_validation_keywords() {
        let service_name = json!({
            "type": "string",
            "minLength": 1
        });
        let plain_string = json!({
            "type": "string"
        });

        let merged = merge_two_schemas(service_name, plain_string);
        assert_eq!(merged, json!({ "type": "string", "minLength": 1 }));

        let merged_nullable = merge_two_schemas(
            json!({ "type": "string", "format": "byte" }),
            json!({ "type": "string", "minLength": 1 }),
        );
        assert_eq!(
            merged_nullable,
            json!({ "type": "string", "format": "byte", "minLength": 1 })
        );
    }
}
