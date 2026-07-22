use serde_json::Value;

/// Internal sibling marker used to preserve "replace this subtree"
/// intent across override reference preparation.
///
/// The override loader sets `$ref-replace` next to every `$ref` it
/// finds *before* refs are bundled or fully inlined. The sibling survives that
/// preparation and signals to the merge that the prepared content should swap
/// into the base, not deep-merge with whatever helm-schema's inference
/// produced for the same path. The marker is stripped from the final output.
pub const REPLACE_MARKER: &str = "$ref-replace";

/// Walk the override and tag every object with `$ref` as
/// "replace on merge". Run by the CLI before reference preparation so the
/// marker rides through bundling or dereferencing and reaches the merge.
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

/// Merges an override into a base schema using replacement markers and schema-aware recursion.
#[must_use]
pub fn apply_schema_override(base: Value, override_schema: Value) -> Value {
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
    //      when the CLI prepares override refs. Bundled refs may remain as
    //      refs and fully inlined refs are gone, but the marker carries the
    //      same replacement intent across both preparation modes.
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
                base_obj.insert(k, apply_schema_override(bv, ov));
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
#[path = "tests/schema_override.rs"]
mod tests;
