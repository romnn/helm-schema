use super::{REPLACE_MARKER, apply_schema_override, mark_refs_for_replacement};
use test_util::prelude::sim_assert_eq;

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

    sim_assert_eq!(have: actual, want: expected);
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

    sim_assert_eq!(have: actual, want: expected);
}

#[test]
fn replace_marker_drives_subtree_replacement_after_reference_preparation() {
    // Models the CLI's actual flow: an override carrying `$ref` is
    // marked, prepared (resolving or re-homing the `$ref` so the
    // marker plus prepared content remain), then merged.
    let base = serde_json::json!({
        "type": "object",
        "properties": {
            "cloud": {
                "anyOf": [{"type": "boolean"}, {"type": "string"}]
            }
        }
    });

    // Prepared override: `$ref` is gone or rewritten elsewhere, but
    // the replace marker survives next to the prepared fields.
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

    sim_assert_eq!(have: actual, want: expected);
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

    sim_assert_eq!(have: actual, want: expected);
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

    sim_assert_eq!(
        have: value["properties"]["cloud"][REPLACE_MARKER],
        want: serde_json::json!(true)
    );
    sim_assert_eq!(
        have: value["properties"]["appV"][REPLACE_MARKER],
        want: serde_json::json!(true)
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

    sim_assert_eq!(have: actual, want: expected);
}
