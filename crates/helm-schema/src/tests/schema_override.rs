use super::{UnpreparedOverride, apply_prepared_schema_override, apply_schema_override};
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
fn prepared_reference_intent_replaces_the_inferred_subtree() {
    let base = serde_json::json!({
        "type": "object",
        "properties": {
            "cloud": {
                "anyOf": [{"type": "boolean"}, {"type": "string"}]
            }
        }
    });

    let unprepared = UnpreparedOverride::capture(serde_json::json!({
        "properties": {
            "cloud": {
                "$ref": "./cloud.json"
            }
        }
    }));
    let prepared = unprepared.into_prepared(serde_json::json!({
        "properties": {
            "cloud": {
                "enum": [null, "azure", "minikube"]
            }
        }
    }));

    let actual = apply_prepared_schema_override(base, prepared);

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
fn caller_authored_ref_replace_keys_are_not_merge_controls() {
    let base = serde_json::json!({
        "properties": {
            "cloud": {
                "type": "object",
                "properties": {"existing": {"type": "string"}}
            }
        }
    });
    let ov = serde_json::json!({
        "properties": {
            "cloud": {
                "$ref-replace": "caller value",
                "properties": {"added": {"type": "integer"}}
            }
        }
    });

    let actual = apply_schema_override(base, ov);

    let expected = serde_json::json!({
        "properties": {
            "cloud": {
                "$ref-replace": "caller value",
                "type": "object",
                "properties": {
                    "added": {"type": "integer"},
                    "existing": {"type": "string"}
                }
            }
        }
    });

    sim_assert_eq!(have: actual, want: expected);
}

#[test]
fn caller_authored_ref_replace_key_survives_reference_replacement() {
    let base = serde_json::json!({
        "properties": {"cloud": {"type": "string"}}
    });
    let unprepared = UnpreparedOverride::capture(serde_json::json!({
        "properties": {
            "cloud": {
                "$ref": "./cloud.json",
                "$ref-replace": "caller value"
            }
        }
    }));
    let prepared = unprepared.into_prepared(serde_json::json!({
        "properties": {
            "cloud": {
                "$ref-replace": "caller value",
                "enum": ["azure", "minikube"]
            }
        }
    }));

    let actual = apply_prepared_schema_override(base, prepared);

    sim_assert_eq!(
        have: actual,
        want: serde_json::json!({
            "properties": {
                "cloud": {
                    "$ref-replace": "caller value",
                    "enum": ["azure", "minikube"]
                }
            }
        })
    );
}
