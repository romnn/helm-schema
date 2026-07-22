use serde_json::Value;
use test_util::prelude::sim_assert_eq;

use super::mirror_global_schema_into_subcharts;

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the full nested schema fixture keeps this transformation regression readable"
)]
fn shared_global_override_schema_is_mirrored_into_nested_subcharts() {
    let mut schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "global": {
                "additionalProperties": true,
                "properties": {
                    "kube-score/ignore": {
                        "type": "string"
                    }
                },
                "type": "object"
            },
            "oauth2-proxy": {
                "additionalProperties": false,
                "properties": {
                    "global": {
                        "additionalProperties": false,
                        "properties": {
                            "imageRegistry": {
                                "type": "string"
                            }
                        },
                        "type": "object"
                    },
                    "redis": {
                        "additionalProperties": false,
                        "properties": {
                            "global": {
                                "additionalProperties": false,
                                "properties": {
                                    "storageClass": {
                                        "type": "string"
                                    }
                                },
                                "type": "object"
                            }
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            }
        },
        "type": "object"
    });

    mirror_global_schema_into_subcharts(
        &mut schema,
        &[
            vec!["oauth2-proxy".to_string()],
            vec!["oauth2-proxy".to_string(), "redis".to_string()],
        ],
    );

    let child_global = schema
        .pointer("/properties/oauth2-proxy/properties/global")
        .expect("child global schema");
    sim_assert_eq!(
        have: child_global
            .pointer("/properties/kube-score~1ignore/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "shared global property should be mirrored into child global: {child_global}"
    );
    sim_assert_eq!(
        have: child_global
            .pointer("/properties/imageRegistry/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "child global-specific properties should be preserved: {child_global}"
    );
    sim_assert_eq!(
        have: child_global
            .get("additionalProperties")
            .and_then(Value::as_bool),
        want: Some(true),
        "shared open-global policy should be mirrored into child global: {child_global}"
    );

    let nested_global = schema
        .pointer("/properties/oauth2-proxy/properties/redis/properties/global")
        .expect("nested global schema");
    sim_assert_eq!(
        have: nested_global
            .pointer("/properties/kube-score~1ignore/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "shared global property should be mirrored into nested child global: {nested_global}"
    );
    sim_assert_eq!(
        have: nested_global
            .pointer("/properties/storageClass/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "nested child global-specific properties should be preserved: {nested_global}"
    );
    sim_assert_eq!(
        have: nested_global
            .get("additionalProperties")
            .and_then(Value::as_bool),
        want: Some(true),
        "shared open-global policy should be mirrored into nested child global: {nested_global}"
    );
}
