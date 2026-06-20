use serde_json::Value;

pub(super) fn strip_schema_descriptions(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };

    object.remove("description");

    for key in [
        "additionalItems",
        "additionalProperties",
        "contains",
        "else",
        "if",
        "not",
        "propertyNames",
        "then",
        "unevaluatedItems",
        "unevaluatedProperties",
    ] {
        if let Some(child) = object.get_mut(key) {
            strip_schema_descriptions(child);
        }
    }

    if let Some(items) = object.get_mut("items") {
        strip_schema_or_schema_array_descriptions(items);
    }

    for key in [
        "$defs",
        "definitions",
        "dependentSchemas",
        "dependencies",
        "patternProperties",
        "properties",
    ] {
        if let Some(Value::Object(children)) = object.get_mut(key) {
            for child in children.values_mut() {
                strip_schema_descriptions(child);
            }
        }
    }

    for key in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(Value::Array(children)) = object.get_mut(key) {
            for child in children {
                strip_schema_descriptions(child);
            }
        }
    }
}

fn strip_schema_or_schema_array_descriptions(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                strip_schema_descriptions(item);
            }
        }
        value => strip_schema_descriptions(value),
    }
}

#[cfg(test)]
mod tests {
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
}
