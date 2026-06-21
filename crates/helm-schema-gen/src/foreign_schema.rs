use serde_json::{Map, Value};

const ARRAY_KEYWORDS: &[&str] = &[
    "additionalItems",
    "contains",
    "items",
    "maxItems",
    "minItems",
    "prefixItems",
    "uniqueItems",
];

const NON_SCALAR_KEYWORDS: &[&str] = &[
    "additionalItems",
    "additionalProperties",
    "contains",
    "items",
    "maxItems",
    "maxProperties",
    "minItems",
    "minProperties",
    "patternProperties",
    "prefixItems",
    "properties",
    "propertyNames",
    "required",
    "uniqueItems",
];

const OBJECT_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "maxProperties",
    "minProperties",
    "patternProperties",
    "properties",
    "propertyNames",
    "required",
];

#[derive(Debug)]
pub(crate) struct ForeignSchemaObject {
    raw: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ForeignSchemaTypeField {
    Single(String),
    Multiple(Vec<String>),
    Absent,
    Unsupported,
}

impl ForeignSchemaObject {
    pub(crate) fn from_value(value: Value) -> Result<Self, Value> {
        match value {
            Value::Object(raw) => Ok(Self { raw }),
            other => Err(other),
        }
    }

    pub(crate) fn union_variants(&self, key: &str) -> Option<Vec<Value>> {
        self.raw.get(key).and_then(Value::as_array).cloned()
    }

    pub(crate) fn type_field(&self) -> ForeignSchemaTypeField {
        match self.raw.get("type") {
            Some(Value::String(schema_type)) => ForeignSchemaTypeField::Single(schema_type.clone()),
            Some(Value::Array(schema_types)) => {
                let mut values = Vec::with_capacity(schema_types.len());
                for schema_type in schema_types {
                    let Some(schema_type) = schema_type.as_str() else {
                        return ForeignSchemaTypeField::Unsupported;
                    };
                    values.push(schema_type.to_string());
                }
                ForeignSchemaTypeField::Multiple(values)
            }
            Some(_) => ForeignSchemaTypeField::Unsupported,
            None => ForeignSchemaTypeField::Absent,
        }
    }

    pub(crate) fn allows_type(&self, expected: &str) -> bool {
        match self.type_field() {
            ForeignSchemaTypeField::Single(schema_type) => schema_type == expected,
            ForeignSchemaTypeField::Multiple(schema_types) => schema_types
                .into_iter()
                .any(|schema_type| schema_type == expected),
            ForeignSchemaTypeField::Unsupported => false,
            ForeignSchemaTypeField::Absent => {
                expected == "array" && self.has_any_keywords(ARRAY_KEYWORDS)
            }
        }
    }

    pub(crate) fn has_non_scalar_keywords(&self) -> bool {
        self.has_any_keywords(NON_SCALAR_KEYWORDS)
    }

    pub(crate) fn take_items(&mut self) -> Option<Value> {
        self.raw.remove("items")
    }

    pub(crate) fn set_items(&mut self, items: Value) {
        self.raw.insert("items".to_string(), items);
    }

    pub(crate) fn set_type_string(&mut self, schema_type: &str) {
        self.raw
            .insert("type".to_string(), Value::String(schema_type.to_string()));
    }

    pub(crate) fn set_type_variants(&mut self, schema_types: Vec<String>) {
        self.raw.insert(
            "type".to_string(),
            Value::Array(schema_types.into_iter().map(Value::String).collect()),
        );
    }

    pub(crate) fn strip_non_scalar_keywords(&mut self) {
        self.remove_keywords(NON_SCALAR_KEYWORDS);
    }

    pub(crate) fn strip_object_keywords(&mut self) {
        self.remove_keywords(OBJECT_KEYWORDS);
    }

    pub(crate) fn into_annotations_only(mut self) -> Self {
        self.raw.retain(|key, _| is_schema_annotation_keyword(key));
        self
    }

    pub(crate) fn set_keyword(&mut self, key: &str, value: Value) {
        self.raw.insert(key.to_string(), value);
    }

    pub(crate) fn into_value(self) -> Value {
        Value::Object(self.raw)
    }

    fn has_any_keywords(&self, keys: &[&str]) -> bool {
        keys.iter().any(|key| self.raw.contains_key(*key))
    }

    fn remove_keywords(&mut self, keys: &[&str]) {
        for key in keys {
            self.raw.remove(*key);
        }
    }
}

fn is_schema_annotation_keyword(key: &str) -> bool {
    matches!(
        key,
        "description" | "title" | "default" | "examples" | "deprecated" | "readOnly" | "writeOnly"
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use test_util::prelude::sim_assert_eq;

    use super::{ForeignSchemaObject, ForeignSchemaTypeField};

    #[test]
    fn allows_array_from_explicit_type_array() {
        let schema = ForeignSchemaObject::from_value(json!({
            "type": ["array", "object"]
        }))
        .expect("object schema");

        assert!(schema.allows_type("array"));
        assert!(schema.allows_type("object"));
        assert!(!schema.allows_type("string"));
    }

    #[test]
    fn allows_array_from_array_keywords_without_type_field() {
        let schema = ForeignSchemaObject::from_value(json!({
            "items": { "type": "string" }
        }))
        .expect("object schema");

        assert!(schema.allows_type("array"));
        assert!(!schema.allows_type("object"));
    }

    #[test]
    fn annotations_only_drops_structural_keywords() {
        let schema = ForeignSchemaObject::from_value(json!({
            "description": "provider leaf",
            "type": "array",
            "items": { "type": "string" }
        }))
        .expect("object schema");

        sim_assert_eq!(
            have: schema.into_annotations_only().into_value(),
            want: json!({
                "description": "provider leaf"
            })
        );
    }

    #[test]
    fn type_field_reports_supported_variants() {
        let schema = ForeignSchemaObject::from_value(json!({
            "type": ["string", "null"]
        }))
        .expect("object schema");

        sim_assert_eq!(
            have: schema.type_field(),
            want: ForeignSchemaTypeField::Multiple(vec!["string".to_string(), "null".to_string()])
        );
    }

    #[test]
    fn type_field_rejects_non_string_type_entries() {
        let schema = ForeignSchemaObject::from_value(json!({
            "type": ["string", 7]
        }))
        .expect("object schema");

        sim_assert_eq!(have: schema.type_field(), want: ForeignSchemaTypeField::Unsupported);
    }
}
