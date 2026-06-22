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
#[path = "tests/descriptions.rs"]
mod tests;
