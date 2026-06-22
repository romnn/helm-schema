use serde_json::Value;
use test_util::prelude::sim_assert_eq;

use super::strip_schema_descriptions;

#[test]
fn strip_schema_descriptions_preserves_description_value_property() {
    let mut schema = serde_json::json!({
        "description": "root annotation",
        "type": "object",
        "properties": {
            "description": {
                "description": "value property annotation",
                "type": "string"
            },
            "nested": {
                "description": "nested annotation",
                "type": "object",
                "properties": {
                    "description": {
                        "description": "nested value property annotation",
                        "type": "string"
                    }
                }
            }
        },
        "$defs": {
            "shared": {
                "description": "shared annotation",
                "type": "string"
            }
        },
        "items": [
            {
                "description": "tuple item annotation",
                "type": "string"
            }
        ]
    });

    strip_schema_descriptions(&mut schema);

    assert!(schema.get("description").is_none());
    sim_assert_eq!(
        have: schema.pointer("/properties/description/type"),
        want: Some(&Value::String("string".to_string())),
    );
    assert!(
        schema
            .pointer("/properties/description/description")
            .is_none(),
    );
    sim_assert_eq!(
        have: schema.pointer("/properties/nested/properties/description/type"),
        want: Some(&Value::String("string".to_string())),
    );
    assert!(
        schema
            .pointer("/properties/nested/properties/description/description")
            .is_none(),
    );
    assert!(schema.pointer("/$defs/shared/description").is_none());
    sim_assert_eq!(
        have: schema.pointer("/items/0/type"),
        want: Some(&Value::String("string".to_string())),
    );
    assert!(schema.pointer("/items/0/description").is_none());
}
