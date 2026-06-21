use serde_json::{Map, Value};

use crate::schema_node::JsonSchemaType;

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

#[derive(Debug, Clone, Copy)]
pub(crate) enum ForeignSchemaRestriction {
    Scalar,
    ScalarCollection,
}

#[derive(Debug)]
struct ForeignSchemaObject {
    raw: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ForeignSchemaTypeField {
    Single(JsonSchemaType),
    Multiple(Vec<JsonSchemaType>),
    Absent,
    Unsupported,
}

#[derive(Debug, Clone, Copy)]
enum ForeignSchemaUnionKind {
    AnyOf,
    OneOf,
    AllOf,
}

impl ForeignSchemaObject {
    fn from_value(value: Value) -> Result<Self, Value> {
        match value {
            Value::Object(raw) => Ok(Self { raw }),
            other => Err(other),
        }
    }

    fn first_union(&self) -> Option<(ForeignSchemaUnionKind, Vec<Value>)> {
        [
            ForeignSchemaUnionKind::AnyOf,
            ForeignSchemaUnionKind::OneOf,
            ForeignSchemaUnionKind::AllOf,
        ]
        .into_iter()
        .find_map(|kind| {
            self.raw
                .get(kind.keyword())
                .and_then(Value::as_array)
                .cloned()
                .map(|variants| (kind, variants))
        })
    }

    fn type_field(&self) -> ForeignSchemaTypeField {
        match self.raw.get("type") {
            Some(Value::String(schema_type)) => JsonSchemaType::from_name(schema_type)
                .map(ForeignSchemaTypeField::Single)
                .unwrap_or(ForeignSchemaTypeField::Unsupported),
            Some(Value::Array(schema_types)) => {
                let mut values = Vec::with_capacity(schema_types.len());
                for schema_type in schema_types {
                    let Some(schema_type) = schema_type.as_str() else {
                        return ForeignSchemaTypeField::Unsupported;
                    };
                    let Some(schema_type) = JsonSchemaType::from_name(schema_type) else {
                        return ForeignSchemaTypeField::Unsupported;
                    };
                    values.push(schema_type);
                }
                ForeignSchemaTypeField::Multiple(values)
            }
            Some(_) => ForeignSchemaTypeField::Unsupported,
            None => ForeignSchemaTypeField::Absent,
        }
    }

    fn allows_type(&self, expected: JsonSchemaType) -> bool {
        match self.type_field() {
            ForeignSchemaTypeField::Single(schema_type) => schema_type == expected,
            ForeignSchemaTypeField::Multiple(schema_types) => schema_types
                .into_iter()
                .any(|schema_type| schema_type == expected),
            ForeignSchemaTypeField::Unsupported => false,
            ForeignSchemaTypeField::Absent => {
                expected == JsonSchemaType::Array && self.has_any_keywords(ARRAY_KEYWORDS)
            }
        }
    }

    fn has_non_scalar_keywords(&self) -> bool {
        self.has_any_keywords(NON_SCALAR_KEYWORDS)
    }

    fn take_items(&mut self) -> Option<Value> {
        self.raw.remove("items")
    }

    fn set_items(&mut self, items: Value) {
        self.raw.insert("items".to_string(), items);
    }

    fn set_type(&mut self, schema_type: JsonSchemaType) {
        self.raw.insert(
            "type".to_string(),
            Value::String(schema_type.as_str().to_string()),
        );
    }

    fn set_type_variants(&mut self, schema_types: Vec<JsonSchemaType>) {
        self.raw.insert(
            "type".to_string(),
            Value::Array(
                schema_types
                    .into_iter()
                    .map(|schema_type| Value::String(schema_type.as_str().to_string()))
                    .collect(),
            ),
        );
    }

    fn strip_non_scalar_keywords(&mut self) {
        self.remove_keywords(NON_SCALAR_KEYWORDS);
    }

    fn strip_object_keywords(&mut self) {
        self.remove_keywords(OBJECT_KEYWORDS);
    }

    fn into_annotations_only(mut self) -> Self {
        self.raw.retain(|key, _| is_schema_annotation_keyword(key));
        self
    }

    fn set_keyword(&mut self, key: &str, value: Value) {
        self.raw.insert(key.to_string(), value);
    }

    fn into_value(self) -> Value {
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

impl ForeignSchemaRestriction {
    pub(crate) fn apply(self, schema: Value) -> Option<Value> {
        match ForeignSchemaObject::from_value(schema) {
            Ok(schema) => self.apply_object(schema),
            Err(other) => match self {
                Self::Scalar => Some(other),
                Self::ScalarCollection => None,
            },
        }
    }

    fn apply_object(self, mut schema: ForeignSchemaObject) -> Option<Value> {
        if let Some((kind, variants)) = schema.first_union() {
            return match kind {
                ForeignSchemaUnionKind::AllOf => {
                    let restricted = variants
                        .into_iter()
                        .map(|variant| self.apply(variant))
                        .collect::<Option<Vec<_>>>()?;
                    schema.set_keyword(kind.keyword(), Value::Array(restricted));
                    Some(schema.into_value())
                }
                ForeignSchemaUnionKind::AnyOf | ForeignSchemaUnionKind::OneOf => {
                    restrict_schema_union(schema, kind, variants, |variant| self.apply(variant))
                }
            };
        }

        match self {
            Self::Scalar => self.apply_scalar_object(schema),
            Self::ScalarCollection => self.apply_scalar_collection_object(schema),
        }
    }

    fn apply_scalar_object(self, mut schema: ForeignSchemaObject) -> Option<Value> {
        if schema.allows_type(JsonSchemaType::Array) {
            return rewrite_array_schema(schema, Self::Scalar);
        }

        match schema.type_field() {
            ForeignSchemaTypeField::Single(schema_type) => {
                scalar_json_type(schema_type).then(|| schema.into_value())
            }
            ForeignSchemaTypeField::Multiple(schema_types) => {
                let scalar_types: Vec<JsonSchemaType> = schema_types
                    .into_iter()
                    .filter(|schema_type| scalar_json_type(*schema_type))
                    .collect();
                if scalar_types.is_empty() {
                    return None;
                }
                if let ForeignSchemaTypeField::Multiple(original_types) = schema.type_field()
                    && scalar_types.len() != original_types.len()
                {
                    schema.set_type_variants(scalar_types);
                    schema.strip_non_scalar_keywords();
                }
                Some(schema.into_value())
            }
            ForeignSchemaTypeField::Absent if schema.has_non_scalar_keywords() => None,
            ForeignSchemaTypeField::Absent => Some(schema.into_value()),
            ForeignSchemaTypeField::Unsupported => Some(schema.into_value()),
        }
    }

    fn apply_scalar_collection_object(self, schema: ForeignSchemaObject) -> Option<Value> {
        if !schema.allows_type(JsonSchemaType::Array) {
            return None;
        }
        rewrite_array_schema(schema, Self::Scalar)
    }
}

impl ForeignSchemaUnionKind {
    fn keyword(self) -> &'static str {
        match self {
            Self::AnyOf => "anyOf",
            Self::OneOf => "oneOf",
            Self::AllOf => "allOf",
        }
    }
}

fn is_schema_annotation_keyword(key: &str) -> bool {
    matches!(
        key,
        "description" | "title" | "default" | "examples" | "deprecated" | "readOnly" | "writeOnly"
    )
}

fn rewrite_array_schema(
    mut schema: ForeignSchemaObject,
    item_restriction: ForeignSchemaRestriction,
) -> Option<Value> {
    if let Some(items) = schema.take_items() {
        schema.set_items(item_restriction.apply(items)?);
    }
    schema.set_type(JsonSchemaType::Array);
    schema.strip_object_keywords();
    Some(schema.into_value())
}

fn restrict_schema_union(
    schema: ForeignSchemaObject,
    keyword: ForeignSchemaUnionKind,
    variants: Vec<Value>,
    restrict: impl FnMut(Value) -> Option<Value>,
) -> Option<Value> {
    let retained_variants: Vec<Value> = variants.into_iter().filter_map(restrict).collect();
    if retained_variants.is_empty() {
        return None;
    }

    let mut annotations = schema.into_annotations_only();
    annotations.set_keyword(keyword.keyword(), Value::Array(retained_variants));
    Some(annotations.into_value())
}

fn scalar_json_type(schema_type: JsonSchemaType) -> bool {
    matches!(
        schema_type,
        JsonSchemaType::String
            | JsonSchemaType::Number
            | JsonSchemaType::Integer
            | JsonSchemaType::Boolean
            | JsonSchemaType::Null
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use test_util::prelude::sim_assert_eq;

    use crate::schema_node::JsonSchemaType;

    use super::{ForeignSchemaObject, ForeignSchemaRestriction, ForeignSchemaTypeField};

    #[test]
    fn allows_array_from_explicit_type_array() {
        let schema = ForeignSchemaObject::from_value(json!({
            "type": ["array", "object"]
        }))
        .expect("object schema");

        assert!(schema.allows_type(JsonSchemaType::Array));
        assert!(schema.allows_type(JsonSchemaType::Object));
        assert!(!schema.allows_type(JsonSchemaType::String));
    }

    #[test]
    fn allows_array_from_array_keywords_without_type_field() {
        let schema = ForeignSchemaObject::from_value(json!({
            "items": { "type": "string" }
        }))
        .expect("object schema");

        assert!(schema.allows_type(JsonSchemaType::Array));
        assert!(!schema.allows_type(JsonSchemaType::Object));
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
            want: ForeignSchemaTypeField::Multiple(vec![
                JsonSchemaType::String,
                JsonSchemaType::Null
            ])
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

    #[test]
    fn scalar_restriction_keeps_only_scalar_union_variants_and_annotations() {
        let schema = json!({
            "description": "provider leaf",
            "anyOf": [
                { "type": "string" },
                { "type": "object", "properties": { "name": { "type": "string" } } }
            ]
        });

        sim_assert_eq!(
            have: ForeignSchemaRestriction::Scalar.apply(schema),
            want: Some(json!({
                "description": "provider leaf",
                "anyOf": [{ "type": "string" }]
            })),
        );
    }

    #[test]
    fn scalar_collection_restriction_requires_array_and_restricts_items() {
        let schema = json!({
            "type": ["array", "object"],
            "properties": { "name": { "type": "string" } },
            "items": {
                "anyOf": [
                    { "type": "string" },
                    { "type": "object" }
                ]
            }
        });

        sim_assert_eq!(
            have: ForeignSchemaRestriction::ScalarCollection.apply(schema),
            want: Some(json!({
                "type": "array",
                "items": {
                    "anyOf": [{ "type": "string" }]
                }
            })),
        );
    }
}
