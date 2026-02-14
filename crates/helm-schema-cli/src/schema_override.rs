use serde_json::Value;

pub fn apply_schema_override(base: Value, override_schema: Value) -> Value {
    apply_override_inner(base, override_schema)
}

fn apply_override_inner(base: Value, override_schema: Value) -> Value {
    let (mut base_obj, override_obj) = match (base, override_schema) {
        (Value::Object(base_obj), Value::Object(override_obj)) => (base_obj, override_obj),
        (_, ov) => return ov,
    };

    for (k, ov) in override_obj {
        if k == "$schema" {
            continue;
        }

        match (k.as_str(), base_obj.get(&k).cloned(), ov) {
            ("required", Some(Value::Array(mut a)), Value::Array(b)) => {
                a.extend(b);
                a.sort_by_key(|v| v.as_str().unwrap_or_default().to_string());
                a.dedup();
                base_obj.insert(k, Value::Array(a));
            }
            (_, Some(bv), ov) => {
                base_obj.insert(k, apply_override_inner(bv, ov));
            }
            (_, None, ov) => {
                base_obj.insert(k, ov);
            }
        }
    }

    Value::Object(base_obj)
}

#[cfg(test)]
mod tests {
    use super::apply_schema_override;

    #[test]
    fn override_merges_objects_and_unions_required() {
        let base = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "a": {"type": "string"}
            },
            "required": ["a"]
        });

        let ov = serde_json::json!({
            "properties": {
                "b": {"type": "integer"}
            },
            "required": ["b"]
        });

        let actual = apply_schema_override(base, ov);

        let expected = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "a": {"type": "string"},
                "b": {"type": "integer"}
            },
            "required": ["a", "b"]
        });

        similar_asserts::assert_eq!(actual, expected);
    }
}
