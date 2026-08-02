use std::collections::BTreeSet;

use serde_json::Value;

#[derive(Debug)]
pub(crate) struct UnpreparedOverride {
    schema: Value,
    replace_at: BTreeSet<String>,
}

impl UnpreparedOverride {
    pub(crate) fn capture(schema: Value) -> Self {
        let mut replace_at = BTreeSet::new();
        collect_replacement_pointers(&schema, "", &mut replace_at);
        Self { schema, replace_at }
    }

    pub(crate) fn schema(&self) -> &Value {
        &self.schema
    }

    pub(crate) fn into_prepared(self, schema: Value) -> PreparedOverride {
        PreparedOverride {
            schema,
            replace_at: self.replace_at,
        }
    }

    fn into_prepared_unmodified(self) -> PreparedOverride {
        PreparedOverride {
            schema: self.schema,
            replace_at: self.replace_at,
        }
    }
}

#[derive(Debug)]
pub(crate) struct PreparedOverride {
    schema: Value,
    replace_at: BTreeSet<String>,
}

impl PreparedOverride {
    pub(crate) fn identity(&self) -> Value {
        serde_json::json!({
            "schema": self.schema,
            "replace-at": self.replace_at,
        })
    }
}

/// Merges one raw override into a base schema using schema-aware recursion.
///
/// `$ref`-bearing subtrees replace inferred content. Replacement intent is
/// captured separately from the JSON document, so caller-authored keywords
/// are never repurposed as merge controls.
#[must_use]
pub fn apply_schema_override(base: Value, override_schema: Value) -> Value {
    apply_prepared_schema_override(
        base,
        UnpreparedOverride::capture(override_schema).into_prepared_unmodified(),
    )
}

pub(crate) fn apply_prepared_schema_override(
    base: Value,
    override_schema: PreparedOverride,
) -> Value {
    apply_schema_override_at(
        base,
        override_schema.schema,
        "",
        &override_schema.replace_at,
    )
}

fn apply_schema_override_at(
    base: Value,
    override_schema: Value,
    pointer: &str,
    replace_at: &BTreeSet<String>,
) -> Value {
    if replace_at.contains(pointer) {
        return override_schema;
    }

    let (mut base_obj, override_obj) = match (base, override_schema) {
        (Value::Object(base_obj), Value::Object(override_obj)) => (base_obj, override_obj),
        (_, override_schema) => return override_schema,
    };

    if override_obj.contains_key("anyOf")
        || override_obj.contains_key("oneOf")
        || override_obj.contains_key("allOf")
    {
        return Value::Object(override_obj);
    }

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
                let child_pointer = format!(
                    "{pointer}/{}",
                    helm_schema_json_schema_walk::escape_json_pointer_segment(&k)
                );
                base_obj.insert(
                    k,
                    apply_schema_override_at(bv, ov, &child_pointer, replace_at),
                );
            }
            (_, None, ov) => {
                base_obj.insert(k, ov);
            }
        }
    }

    Value::Object(base_obj)
}

fn collect_replacement_pointers(value: &Value, pointer: &str, replace_at: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if object.contains_key("$ref") {
                replace_at.insert(pointer.to_string());
                return;
            }
            for (key, child) in object {
                let child_pointer = format!(
                    "{pointer}/{}",
                    helm_schema_json_schema_walk::escape_json_pointer_segment(key)
                );
                collect_replacement_pointers(child, &child_pointer, replace_at);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_replacement_pointers(item, &format!("{pointer}/{index}"), replace_at);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "tests/schema_override.rs"]
mod tests;
