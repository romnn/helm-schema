use serde_json::Value;

/// Convenience operations on JSON Schema values.
pub trait JsonSchemaOps {
    /// Get the `type` field as a string.
    fn schema_type(&self) -> Option<&str>;

    /// Get a property schema by name from `properties`.
    fn property(&self, name: &str) -> Option<&Value>;

    /// Get the `items` schema for an array type.
    fn items_schema(&self) -> Option<&Value>;

    /// Check if this schema has a specific type.
    fn is_type(&self, ty: &str) -> bool;

    /// Get all property names from `properties`.
    fn property_names(&self) -> Vec<String>;

    /// Get `required` field entries.
    fn required_fields(&self) -> Vec<String>;

    /// Get `anyOf` alternatives.
    fn any_of(&self) -> Option<&Vec<Value>>;

    /// Get `allOf` alternatives.
    fn all_of(&self) -> Option<&Vec<Value>>;

    /// Get `oneOf` alternatives.
    fn one_of(&self) -> Option<&Vec<Value>>;
}

impl JsonSchemaOps for Value {
    fn schema_type(&self) -> Option<&str> {
        self.as_object()?.get("type")?.as_str()
    }

    fn property(&self, name: &str) -> Option<&Value> {
        self.as_object()?.get("properties")?.as_object()?.get(name)
    }

    fn items_schema(&self) -> Option<&Value> {
        self.as_object()?.get("items")
    }

    fn is_type(&self, ty: &str) -> bool {
        self.schema_type() == Some(ty)
    }

    fn property_names(&self) -> Vec<String> {
        self.as_object()
            .and_then(|o| o.get("properties"))
            .and_then(|p| p.as_object())
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn required_fields(&self) -> Vec<String> {
        self.as_object()
            .and_then(|o| o.get("required"))
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn any_of(&self) -> Option<&Vec<Value>> {
        self.as_object()?.get("anyOf")?.as_array()
    }

    fn all_of(&self) -> Option<&Vec<Value>> {
        self.as_object()?.get("allOf")?.as_array()
    }

    fn one_of(&self) -> Option<&Vec<Value>> {
        self.as_object()?.get("oneOf")?.as_array()
    }
}

/// Descend into a JSON Schema following a dot-separated path.
///
/// Handles `properties`, `items`, `anyOf`/`allOf`/`oneOf`, and `[*]` array
/// suffixes.
pub fn descend_path(schema: &Value, path: &[String]) -> Option<Value> {
    let mut current = schema.clone();
    for seg in path {
        current = descend_one_segment(&current, seg)?;
    }
    Some(current)
}

fn descend_one_segment(schema: &Value, seg: &str) -> Option<Value> {
    // Try anyOf/allOf/oneOf branches.
    for keyword in &["allOf", "anyOf", "oneOf"] {
        if let Some(arr) = schema
            .as_object()
            .and_then(|o| o.get(*keyword))
            .and_then(|v| v.as_array())
        {
            for branch in arr {
                if let Some(result) = descend_one_segment(branch, seg) {
                    return Some(result);
                }
            }
        }
    }

    let (key, is_array_item) = if let Some(k) = seg.strip_suffix("[*]") {
        (k, true)
    } else {
        (seg, false)
    };

    // Object property.
    let mut next = schema
        .as_object()
        .and_then(|o| o.get("properties"))
        .and_then(|p| p.as_object())
        .and_then(|p| p.get(key))
        .cloned()
        .or_else(|| {
            // Map-like (additionalProperties).
            schema
                .as_object()
                .and_then(|o| o.get("additionalProperties"))
                .and_then(|ap| {
                    if ap.is_boolean() {
                        None
                    } else {
                        Some(ap.clone())
                    }
                })
        })?;

    if is_array_item {
        next = next
            .as_object()
            .and_then(|o| o.get("items"))
            .cloned()
            .or_else(|| {
                next.as_object()
                    .and_then(|o| o.get("prefixItems"))
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .cloned()
            })?;
    }

    Some(next)
}

/// Narrow an `anyOf` schema based on the value path name.
///
/// For fields like `*.enabled` → prefer boolean, `replicas` → prefer integer,
/// `labels`/`annotations` → prefer string map.
pub fn strengthen_leaf_schema(value_path: &str, schema: Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return schema;
    };
    let Some(any_of) = obj.get("anyOf").and_then(|v| v.as_array()) else {
        return schema;
    };

    fn has_type(v: &Value, ty: &str) -> bool {
        v.as_object()
            .and_then(|o| o.get("type"))
            .and_then(|t| t.as_str())
            == Some(ty)
    }

    fn is_string_map(v: &Value) -> bool {
        let Some(o) = v.as_object() else {
            return false;
        };
        if o.get("type").and_then(|v| v.as_str()) != Some("object") {
            return false;
        }
        let Some(ap) = o.get("additionalProperties") else {
            return false;
        };
        ap.as_object()
            .and_then(|ap| ap.get("type"))
            .and_then(|t| t.as_str())
            == Some("string")
    }

    let prefer_bool = value_path == "installCRDs"
        || value_path.ends_with(".enabled")
        || value_path.ends_with("Enabled");
    if prefer_bool {
        if let Some(v) = any_of.iter().find(|v| has_type(v, "boolean")) {
            return v.clone();
        }
    }

    let last = value_path.split('.').last().unwrap_or("");
    let prefer_int = matches!(
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
    );
    if prefer_int {
        if let Some(v) = any_of.iter().find(|v| has_type(v, "integer")) {
            return v.clone();
        }
    }

    let prefer_string_map = last == "labels" || last == "annotations";
    if prefer_string_map {
        if let Some(v) = any_of.iter().find(|v| is_string_map(v)) {
            return v.clone();
        }
        if let Some(v) = any_of.iter().find(|v| has_type(v, "object")) {
            return v.clone();
        }
    }

    schema
}
