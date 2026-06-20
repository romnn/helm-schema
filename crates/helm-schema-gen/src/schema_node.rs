use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Number, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsonSchemaType {
    Array,
    Boolean,
    Integer,
    Null,
    Number,
    Object,
    String,
}

impl JsonSchemaType {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "array" => Some(Self::Array),
            "boolean" => Some(Self::Boolean),
            "integer" => Some(Self::Integer),
            "null" => Some(Self::Null),
            "number" => Some(Self::Number),
            "object" => Some(Self::Object),
            "string" => Some(Self::String),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Array => "array",
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Null => "null",
            Self::Number => "number",
            Self::Object => "object",
            Self::String => "string",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SchemaNode {
    Empty,
    Type(JsonSchemaType),
    Typed {
        ty: JsonSchemaType,
        keywords: BTreeMap<String, Value>,
    },
    Object {
        properties: BTreeMap<String, SchemaNode>,
        include_empty_properties: bool,
        required: BTreeSet<String>,
        additional_properties: Option<Box<SchemaNode>>,
        min_properties: Option<u64>,
        max_properties: Option<u64>,
    },
    Array {
        items: Option<Box<SchemaNode>>,
        min_items: Option<u64>,
    },
    Enum(Vec<Value>),
    Const(Value),
    Not(Box<SchemaNode>),
    AllOf(Vec<SchemaNode>),
    AnyOf(Vec<SchemaNode>),
    Foreign(Value),
}

impl SchemaNode {
    pub(crate) fn empty() -> Self {
        Self::Empty
    }

    pub(crate) fn foreign(value: Value) -> Self {
        Self::Foreign(value)
    }

    pub(crate) fn typed(ty: JsonSchemaType) -> Self {
        Self::Type(ty)
    }

    pub(crate) fn type_named(name: &str) -> Self {
        JsonSchemaType::from_name(name)
            .map(Self::typed)
            .unwrap_or_else(|| Self::Foreign(type_name_schema(name)))
    }

    pub(crate) fn typed_keyword(mut self, key: impl Into<String>, value: Value) -> Self {
        let key = key.into();
        match &mut self {
            Self::Type(ty) => {
                let ty = *ty;
                let keywords = BTreeMap::from_iter([(key, value)]);
                Self::Typed { ty, keywords }
            }
            Self::Typed { keywords, .. } => {
                keywords.insert(key, value);
                self
            }
            _ => self,
        }
    }

    pub(crate) fn object() -> Self {
        Self::Object {
            properties: BTreeMap::new(),
            include_empty_properties: false,
            required: BTreeSet::new(),
            additional_properties: None,
            min_properties: None,
            max_properties: None,
        }
    }

    pub(crate) fn closed_object() -> Self {
        Self::object()
            .with_empty_properties()
            .with_additional_properties(Self::foreign(Value::Bool(false)))
    }

    pub(crate) fn property(mut self, key: impl Into<String>, value: SchemaNode) -> Self {
        if let Self::Object { properties, .. } = &mut self {
            properties.insert(key.into(), value);
        }
        self
    }

    pub(crate) fn with_empty_properties(mut self) -> Self {
        if let Self::Object {
            include_empty_properties,
            ..
        } = &mut self
        {
            *include_empty_properties = true;
        }
        self
    }

    pub(crate) fn require(mut self, key: impl Into<String>) -> Self {
        if let Self::Object { required, .. } = &mut self {
            required.insert(key.into());
        }
        self
    }

    pub(crate) fn with_additional_properties(mut self, value: SchemaNode) -> Self {
        if let Self::Object {
            additional_properties,
            ..
        } = &mut self
        {
            *additional_properties = Some(Box::new(value));
        }
        self
    }

    pub(crate) fn min_properties(mut self, min: u64) -> Self {
        if let Self::Object { min_properties, .. } = &mut self {
            *min_properties = Some(min);
        }
        self
    }

    pub(crate) fn max_properties(mut self, max: u64) -> Self {
        if let Self::Object { max_properties, .. } = &mut self {
            *max_properties = Some(max);
        }
        self
    }

    pub(crate) fn array() -> Self {
        Self::Array {
            items: None,
            min_items: None,
        }
    }

    pub(crate) fn items(mut self, value: SchemaNode) -> Self {
        if let Self::Array { items, .. } = &mut self {
            *items = Some(Box::new(value));
        }
        self
    }

    pub(crate) fn min_items(mut self, min: u64) -> Self {
        if let Self::Array { min_items, .. } = &mut self {
            *min_items = Some(min);
        }
        self
    }

    pub(crate) fn enum_values(values: Vec<Value>) -> Self {
        Self::Enum(values)
    }

    pub(crate) fn const_value(value: Value) -> Self {
        Self::Const(value)
    }

    pub(crate) fn not(value: SchemaNode) -> Self {
        Self::Not(Box::new(value))
    }

    pub(crate) fn all_of(values: Vec<SchemaNode>) -> Self {
        match values.len() {
            0 => Self::empty(),
            1 => values.into_iter().next().expect("single allOf value"),
            _ => Self::AllOf(values),
        }
    }

    pub(crate) fn any_of(values: Vec<SchemaNode>) -> Self {
        match values.len() {
            0 => Self::empty(),
            1 => values.into_iter().next().expect("single anyOf value"),
            _ => Self::AnyOf(values),
        }
    }

    pub(crate) fn into_value(self) -> Value {
        match self {
            Self::Empty => Value::Object(Map::new()),
            Self::Type(ty) => type_schema(ty),
            Self::Typed { ty, keywords } => {
                let mut object = type_map(ty);
                object.extend(keywords);
                Value::Object(object)
            }
            Self::Object {
                properties,
                include_empty_properties,
                required,
                additional_properties,
                min_properties,
                max_properties,
            } => {
                let mut object = type_map(JsonSchemaType::Object);
                if include_empty_properties || !properties.is_empty() {
                    object.insert(
                        "properties".to_string(),
                        Value::Object(
                            properties
                                .into_iter()
                                .map(|(key, value)| (key, value.into_value()))
                                .collect(),
                        ),
                    );
                }
                if !required.is_empty() {
                    object.insert(
                        "required".to_string(),
                        Value::Array(required.into_iter().map(Value::String).collect()),
                    );
                }
                if let Some(additional_properties) = additional_properties {
                    object.insert(
                        "additionalProperties".to_string(),
                        additional_properties.into_value(),
                    );
                }
                if let Some(min_properties) = min_properties {
                    object.insert(
                        "minProperties".to_string(),
                        Value::Number(Number::from(min_properties)),
                    );
                }
                if let Some(max_properties) = max_properties {
                    object.insert(
                        "maxProperties".to_string(),
                        Value::Number(Number::from(max_properties)),
                    );
                }
                Value::Object(object)
            }
            Self::Array { items, min_items } => {
                let mut object = type_map(JsonSchemaType::Array);
                if let Some(items) = items {
                    object.insert("items".to_string(), items.into_value());
                }
                if let Some(min_items) = min_items {
                    object.insert(
                        "minItems".to_string(),
                        Value::Number(Number::from(min_items)),
                    );
                }
                Value::Object(object)
            }
            Self::Enum(values) => Value::Object(
                [("enum".to_string(), Value::Array(values))]
                    .into_iter()
                    .collect(),
            ),
            Self::Const(value) => {
                Value::Object([("const".to_string(), value)].into_iter().collect())
            }
            Self::Not(value) => Value::Object(
                [("not".to_string(), value.into_value())]
                    .into_iter()
                    .collect(),
            ),
            Self::AllOf(values) => Value::Object(
                [(
                    "allOf".to_string(),
                    Value::Array(values.into_iter().map(Self::into_value).collect()),
                )]
                .into_iter()
                .collect(),
            ),
            Self::AnyOf(values) => Value::Object(
                [(
                    "anyOf".to_string(),
                    Value::Array(values.into_iter().map(Self::into_value).collect()),
                )]
                .into_iter()
                .collect(),
            ),
            Self::Foreign(value) => value,
        }
    }
}

fn type_name_schema(name: &str) -> Value {
    Value::Object(
        [("type".to_string(), Value::String(name.to_string()))]
            .into_iter()
            .collect(),
    )
}

fn type_schema(ty: JsonSchemaType) -> Value {
    Value::Object(type_map(ty))
}

fn type_map(ty: JsonSchemaType) -> Map<String, Value> {
    Map::from_iter([("type".to_string(), Value::String(ty.as_str().to_string()))])
}
