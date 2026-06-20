use serde_json::{Map, Value};

use crate::schema_override;

pub(super) fn mirror_global_schema_into_subcharts(
    schema: &mut Value,
    subchart_prefixes: &[Vec<String>],
) {
    let Some(root_global_schema) = schema.pointer("/properties/global").cloned() else {
        return;
    };

    for prefix in subchart_prefixes {
        let subchart_schema = schema_object_at_values_prefix(schema, prefix);
        let subchart_global_schema = schema_property_mut(subchart_schema, "global");
        let existing = std::mem::take(subchart_global_schema);
        *subchart_global_schema =
            schema_override::apply_schema_override(existing, root_global_schema.clone());
    }
}

fn schema_object_at_values_prefix<'a>(schema: &'a mut Value, prefix: &[String]) -> &'a mut Value {
    let mut current = schema;
    for segment in prefix {
        current = schema_property_mut(current, segment);
    }
    current
}

fn schema_property_mut<'a>(schema: &'a mut Value, property: &str) -> &'a mut Value {
    let object = ensure_json_object(schema);
    let properties = object
        .entry("properties".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let properties = ensure_json_object(properties);
    properties
        .entry(property.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
}

fn ensure_json_object(value: &mut Value) -> &mut Map<String, Value> {
    match value {
        Value::Object(object) => object,
        _ => {
            *value = Value::Object(Map::new());
            ensure_json_object(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use test_util::prelude::sim_assert_eq;

    use super::mirror_global_schema_into_subcharts;

    #[test]
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
}
