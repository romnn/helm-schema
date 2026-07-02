use serde_json::{Map, Value};

use crate::schema_model::is_annotation_keyword;
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

impl ForeignSchemaObject {
    fn from_value(value: Value) -> Result<Self, Value> {
        match value {
            Value::Object(raw) => Ok(Self { raw }),
            other => Err(other),
        }
    }

    fn first_union(&self) -> Option<(&'static str, Vec<Value>)> {
        ["anyOf", "oneOf", "allOf"].into_iter().find_map(|keyword| {
            self.raw
                .get(keyword)
                .and_then(Value::as_array)
                .cloned()
                .map(|variants| (keyword, variants))
        })
    }

    fn type_variants(&self) -> Result<Option<Vec<JsonSchemaType>>, ()> {
        match self.raw.get("type") {
            Some(Value::String(schema_type)) => JsonSchemaType::from_name(schema_type)
                .map(|schema_type| Some(vec![schema_type]))
                .ok_or(()),
            Some(Value::Array(schema_types)) => {
                let mut values = Vec::with_capacity(schema_types.len());
                for schema_type in schema_types {
                    let Some(schema_type) = schema_type.as_str() else {
                        return Err(());
                    };
                    let Some(schema_type) = JsonSchemaType::from_name(schema_type) else {
                        return Err(());
                    };
                    values.push(schema_type);
                }
                Ok(Some(values))
            }
            Some(_) => Err(()),
            None => Ok(None),
        }
    }

    fn allows_type(&self, expected: JsonSchemaType) -> bool {
        match self.type_variants() {
            Ok(Some(schema_types)) => schema_types.contains(&expected),
            Ok(None) => expected == JsonSchemaType::Array && self.has_any_keywords(ARRAY_KEYWORDS),
            Err(()) => false,
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
        self.raw.retain(|key, _| is_annotation_keyword(key));
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
                "allOf" => {
                    let restricted = variants
                        .into_iter()
                        .map(|variant| self.apply(variant))
                        .collect::<Option<Vec<_>>>()?;
                    schema.set_keyword(kind, Value::Array(restricted));
                    Some(schema.into_value())
                }
                _ => restrict_schema_union(schema, kind, variants, |variant| self.apply(variant)),
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

        match schema.type_variants() {
            Ok(Some(schema_types)) => {
                let scalar_types = schema_types
                    .iter()
                    .copied()
                    .filter(|schema_type| scalar_json_type(*schema_type))
                    .collect::<Vec<_>>();
                if scalar_types.is_empty() {
                    return None;
                }
                if scalar_types.len() != schema_types.len() {
                    schema.set_type_variants(scalar_types);
                    schema.strip_non_scalar_keywords();
                }
                Some(schema.into_value())
            }
            Ok(None) if schema.has_non_scalar_keywords() => None,
            Ok(None) | Err(()) => Some(schema.into_value()),
        }
    }

    fn apply_scalar_collection_object(self, schema: ForeignSchemaObject) -> Option<Value> {
        if !schema.allows_type(JsonSchemaType::Array) {
            return None;
        }
        rewrite_array_schema(schema, Self::Scalar)
    }
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
    keyword: &'static str,
    variants: Vec<Value>,
    restrict: impl FnMut(Value) -> Option<Value>,
) -> Option<Value> {
    let retained_variants: Vec<Value> = variants.into_iter().filter_map(restrict).collect();
    if retained_variants.is_empty() {
        return None;
    }

    let mut annotations = schema.into_annotations_only();
    annotations.set_keyword(keyword, Value::Array(retained_variants));
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
#[path = "tests/foreign_schema.rs"]
mod tests;
