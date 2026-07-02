use serde_json::{Map, Value};

pub(crate) fn enrich_root_metadata_schema(mut root: Value) -> Value {
    let Some(obj) = root.as_object_mut() else {
        return root;
    };
    let Some(properties) = obj.get_mut("properties").and_then(Value::as_object_mut) else {
        return root;
    };

    match properties.get_mut("metadata") {
        Some(metadata) => enrich_metadata_object(metadata),
        None => {
            properties.insert("metadata".to_string(), metadata_object_schema());
        }
    }

    root
}

pub(crate) fn enriched_metadata_schema(root: &Value) -> Value {
    let mut metadata = root
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("metadata"))
        .cloned()
        .unwrap_or_else(metadata_object_schema);
    enrich_metadata_object(&mut metadata);
    metadata
}

fn enrich_metadata_object(metadata: &mut Value) {
    let Some(obj) = metadata.as_object_mut() else {
        *metadata = metadata_object_schema();
        return;
    };

    obj.entry("type".to_string())
        .or_insert_with(|| Value::String("object".to_string()));
    if obj.get("properties").and_then(Value::as_object).is_none() {
        obj.insert("properties".to_string(), Value::Object(Map::new()));
    }

    let Some(properties) = obj.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };

    strengthen_property(properties, "name", string_schema);
    strengthen_property(properties, "namespace", string_schema);
    strengthen_property(properties, "labels", string_map_schema);
    strengthen_property(properties, "annotations", string_map_schema);
}

fn strengthen_property(properties: &mut Map<String, Value>, key: &str, desired: fn() -> Value) {
    match properties.get_mut(key) {
        Some(existing) if is_weak_metadata_leaf(existing) => {
            *existing = desired();
        }
        Some(_) => {}
        None => {
            properties.insert(key.to_string(), desired());
        }
    }
}

fn is_weak_metadata_leaf(schema: &Value) -> bool {
    let Some(obj) = schema.as_object() else {
        return false;
    };
    if obj.is_empty() {
        return true;
    }

    let has_properties = obj
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|map| !map.is_empty());
    if has_properties {
        return false;
    }

    obj.get("additionalProperties")
        .and_then(Value::as_object)
        .is_some_and(Map::is_empty)
}

fn metadata_object_schema() -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            (
                "properties".to_string(),
                Value::Object(
                    [
                        ("name".to_string(), string_schema()),
                        ("namespace".to_string(), string_schema()),
                        ("labels".to_string(), string_map_schema()),
                        ("annotations".to_string(), string_map_schema()),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn string_schema() -> Value {
    Value::Object(
        [("type".to_string(), Value::String("string".to_string()))]
            .into_iter()
            .collect(),
    )
}

fn string_map_schema() -> Value {
    Value::Object(
        [
            ("type".to_string(), Value::String("object".to_string())),
            ("additionalProperties".to_string(), string_schema()),
        ]
        .into_iter()
        .collect(),
    )
}
