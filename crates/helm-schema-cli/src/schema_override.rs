use serde_json::Value;

/// Internal sibling marker used to preserve "replace this subtree"
/// intent across override reference dereferencing.
///
/// The override loader sets `$ref-replace` next to every `$ref` it
/// finds *before* dereferencing. Once `$ref` is resolved and the inlined
/// content replaces the ref node, this sibling survives and signals to
/// the merge that the resolved content should swap into the base, not
/// deep-merge with whatever helm-schema's inference produced for the
/// same path. The marker is stripped from the final output.
pub const REPLACE_MARKER: &str = "$ref-replace";

pub fn apply_schema_override(base: Value, override_schema: Value) -> Value {
    apply_override_inner(base, override_schema)
}

/// Walk the override and tag every object with `$ref` as
/// "replace on merge". Run by the CLI before dereferencing so the
/// marker rides through the dereference pass and reaches the merge.
pub fn mark_refs_for_replacement(value: &mut Value) {
    match value {
        Value::Object(obj) => {
            if obj.contains_key("$ref") {
                obj.insert(REPLACE_MARKER.to_string(), Value::Bool(true));
            }
            for (_, v) in obj.iter_mut() {
                mark_refs_for_replacement(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                mark_refs_for_replacement(v);
            }
        }
        _ => {}
    }
}

fn apply_override_inner(base: Value, override_schema: Value) -> Value {
    let (mut base_obj, mut override_obj) = match (base, override_schema) {
        (Value::Object(base_obj), Value::Object(override_obj)) => (base_obj, override_obj),
        (_, ov) => return strip_replace_markers(ov),
    };

    // Two replacement signals collapse to the same semantic: an override
    // subtree must swap into the base, not deep-merge with it.
    //   1. Raw `$ref` — JSON Schema draft-07 ignores its siblings, so an
    //      inferred `type`/`properties` left in the base would either
    //      contradict the refed content (e.g. inferred
    //      `cloud: {type: [boolean, string]}` vs an override
    //      `cloud: {$ref: ./cloud.json}` whose enum includes `null`) or
    //      survive into the output where the JSON Schema spec said they
    //      shouldn't.
    //   2. `REPLACE_MARKER` left behind by `mark_refs_for_replacement`
    //      when the CLI dereferences overrides — the `$ref` is gone by
    //      then, but the marker carries the same intent across the
    //      dereference pass.
    let had_replace_marker = override_obj.remove(REPLACE_MARKER).is_some();
    if override_obj.contains_key("$ref") || had_replace_marker {
        return Value::Object(override_obj);
    }
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
                base_obj.insert(k, apply_override_inner(bv, ov));
            }
            (_, None, ov) => {
                base_obj.insert(k, strip_replace_markers(ov));
            }
        }
    }

    Value::Object(base_obj)
}

fn strip_replace_markers(value: Value) -> Value {
    match value {
        Value::Object(mut obj) => {
            obj.remove(REPLACE_MARKER);
            let mapped = obj
                .into_iter()
                .map(|(k, v)| (k, strip_replace_markers(v)))
                .collect();
            Value::Object(mapped)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(strip_replace_markers).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{REPLACE_MARKER, apply_schema_override, mark_refs_for_replacement};

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

    #[test]
    fn override_with_ref_replaces_base_subtree() {
        let base = serde_json::json!({
            "type": "object",
            "properties": {
                "cloud": { "type": ["boolean", "string"] }
            }
        });

        let ov = serde_json::json!({
            "properties": {
                "cloud": { "$ref": "./cloud.json" }
            }
        });

        let actual = apply_schema_override(base, ov);

        let expected = serde_json::json!({
            "type": "object",
            "properties": {
                "cloud": { "$ref": "./cloud.json" }
            }
        });

        similar_asserts::assert_eq!(actual, expected);
    }

    #[test]
    fn replace_marker_drives_subtree_replacement_after_dereference() {
        // Models the CLI's actual flow: an override carrying `$ref` is
        // marked, dereferenced (resolving the `$ref` so only the
        // marker plus resolved content remain), then merged.
        let base = serde_json::json!({
            "type": "object",
            "properties": {
                "cloud": {
                    "anyOf": [{"type": "boolean"}, {"type": "string"}]
                }
            }
        });

        // Dereferenced override: $ref is gone, content is inlined, but
        // the replace marker survives next to the inlined fields.
        let ov = serde_json::json!({
            "properties": {
                "cloud": {
                    REPLACE_MARKER: true,
                    "enum": [null, "azure", "minikube"]
                }
            }
        });

        let actual = apply_schema_override(base, ov);

        let expected = serde_json::json!({
            "type": "object",
            "properties": {
                "cloud": {
                    "enum": [null, "azure", "minikube"]
                }
            }
        });

        similar_asserts::assert_eq!(actual, expected);
    }

    #[test]
    fn composition_override_replaces_conflicting_scalar_constraints() {
        let base = serde_json::json!({
            "type": "object",
            "properties": {
                "imageRegistry": {
                    "type": "string"
                }
            }
        });

        let ov = serde_json::json!({
            "properties": {
                "imageRegistry": {
                    "anyOf": [
                        { "type": "null" },
                        { "type": "string" }
                    ]
                }
            }
        });

        let actual = apply_schema_override(base, ov);

        let expected = serde_json::json!({
            "type": "object",
            "properties": {
                "imageRegistry": {
                    "anyOf": [
                        { "type": "null" },
                        { "type": "string" }
                    ]
                }
            }
        });

        similar_asserts::assert_eq!(actual, expected);
    }

    #[test]
    fn mark_refs_tags_only_ref_carrying_subtrees() {
        let mut value = serde_json::json!({
            "properties": {
                "cloud":    { "$ref": "./cloud.json" },
                "appV":     { "$ref": "./v.json" },
                "common":   { "type": "object", "additionalProperties": true }
            }
        });

        mark_refs_for_replacement(&mut value);

        assert_eq!(
            value["properties"]["cloud"][REPLACE_MARKER],
            serde_json::json!(true)
        );
        assert_eq!(
            value["properties"]["appV"][REPLACE_MARKER],
            serde_json::json!(true)
        );
        assert!(value["properties"]["common"].get(REPLACE_MARKER).is_none());
    }

    #[test]
    fn replace_markers_in_inserted_subtrees_are_stripped() {
        // When the override adds a property that doesn't exist in the
        // base, the inserted content still has its markers cleaned up.
        let base = serde_json::json!({ "type": "object", "properties": {} });
        let ov = serde_json::json!({
            "properties": {
                "cloud": {
                    REPLACE_MARKER: true,
                    "enum": [null, "azure"]
                }
            }
        });

        let actual = apply_schema_override(base, ov);

        let expected = serde_json::json!({
            "type": "object",
            "properties": {
                "cloud": { "enum": [null, "azure"] }
            }
        });

        similar_asserts::assert_eq!(actual, expected);
    }
}
