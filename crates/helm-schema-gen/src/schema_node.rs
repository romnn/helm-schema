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

    pub(crate) fn as_str(self) -> &'static str {
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
    Object {
        properties: BTreeMap<String, SchemaNode>,
        /// Whether the emitted schema pins `type: object`. Hosts that only
        /// exist to carry a referenced descendant stay untyped: member
        /// presence is conditional evidence, not a shape claim.
        typed: bool,
        all_of: Vec<SchemaNode>,
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
    Typed(TypedSchemaNode),
    Foreign(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TypedSchemaNode {
    Boolean(bool),
    Keywords(Box<SchemaKeywords>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SchemaTypeKeyword {
    Single(JsonSchemaType),
    Multiple(Vec<JsonSchemaType>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SchemaKeywords {
    schema_type: Option<SchemaTypeKeyword>,
    properties: Option<BTreeMap<String, SchemaNode>>,
    required: Option<Vec<String>>,
    additional_properties: Option<Box<SchemaNode>>,
    items: Option<Box<SchemaNode>>,
    all_of: Option<Vec<SchemaNode>>,
    any_of: Option<Vec<SchemaNode>>,
    one_of: Option<Vec<SchemaNode>>,
    not: Option<Box<SchemaNode>>,
    if_schema: Option<Box<SchemaNode>>,
    then_schema: Option<Box<SchemaNode>>,
    else_schema: Option<Box<SchemaNode>>,
    min_properties: Option<u64>,
    max_properties: Option<u64>,
    min_items: Option<u64>,
    extra_keywords: BTreeMap<String, Value>,
}

pub(crate) fn is_placeholder_fragment_object_schema(schema: &Value) -> bool {
    schema.as_object().is_some_and(|object| {
        object.get("type").and_then(Value::as_str) == Some("object")
            && matches!(
                object.get("additionalProperties"),
                Some(Value::Object(additional_properties)) if additional_properties.is_empty()
            )
            && !object.contains_key("properties")
            && !object.contains_key("required")
    })
}

impl SchemaNode {
    pub(crate) fn visit_foreign_values(&self, visit: &mut impl FnMut(&Value)) {
        match self {
            Self::Empty => {}
            Self::Object {
                properties,
                all_of,
                additional_properties,
                ..
            } => {
                for child in properties.values() {
                    child.visit_foreign_values(visit);
                }
                for child in all_of {
                    child.visit_foreign_values(visit);
                }
                if let Some(child) = additional_properties {
                    child.visit_foreign_values(visit);
                }
            }
            Self::Array { items, .. } => {
                if let Some(items) = items {
                    items.visit_foreign_values(visit);
                }
            }
            Self::Typed(schema) => schema.visit_embedded_values(visit),
            Self::Foreign(value) => visit(value),
        }
    }

    pub(crate) fn empty() -> Self {
        Self::Empty
    }

    pub(crate) fn foreign(value: Value) -> Self {
        Self::Foreign(value)
    }

    pub(crate) fn from_value(value: Value) -> Self {
        match value {
            Value::Bool(value) => Self::Typed(TypedSchemaNode::Boolean(value)),
            Value::Object(mut object) => {
                let schema_type = take_schema_type(&mut object);
                let properties = take_object_keyword(&mut object, "properties").map(|properties| {
                    properties
                        .into_iter()
                        .map(|(key, value)| (key, Self::from_value(value)))
                        .collect()
                });
                let required = take_string_array_keyword(&mut object, "required");
                let additional_properties =
                    take_schema_keyword(&mut object, "additionalProperties");
                let items = take_schema_keyword(&mut object, "items");
                let all_of = take_schema_array_keyword(&mut object, "allOf");
                let any_of = take_schema_array_keyword(&mut object, "anyOf");
                let one_of = take_schema_array_keyword(&mut object, "oneOf");
                let not = take_schema_keyword(&mut object, "not");
                let if_schema = take_schema_keyword(&mut object, "if");
                let then_schema = take_schema_keyword(&mut object, "then");
                let else_schema = take_schema_keyword(&mut object, "else");
                let min_properties = take_u64_keyword(&mut object, "minProperties");
                let max_properties = take_u64_keyword(&mut object, "maxProperties");
                let min_items = take_u64_keyword(&mut object, "minItems");

                Self::Typed(TypedSchemaNode::Keywords(Box::new(SchemaKeywords {
                    schema_type,
                    properties,
                    required,
                    additional_properties,
                    items,
                    all_of,
                    any_of,
                    one_of,
                    not,
                    if_schema,
                    then_schema,
                    else_schema,
                    min_properties,
                    max_properties,
                    min_items,
                    extra_keywords: object.into_iter().collect(),
                })))
            }
            value => Self::Foreign(value),
        }
    }

    pub(crate) fn typed(ty: JsonSchemaType) -> Self {
        Self::Foreign(Value::Object(type_map(ty)))
    }

    pub(crate) fn type_named(name: &str) -> Self {
        Self::keyword_schema("type", Value::String(name.to_string()))
    }

    pub(crate) fn typed_keyword(mut self, key: impl Into<String>, value: Value) -> Self {
        let key = key.into();
        match &mut self {
            Self::Foreign(Value::Object(object)) => {
                object.insert(key, value);
                self
            }
            _ => self,
        }
    }

    pub(crate) fn object() -> Self {
        Self::Object {
            properties: BTreeMap::new(),
            typed: true,
            all_of: Vec::new(),
            include_empty_properties: false,
            required: BTreeSet::new(),
            additional_properties: None,
            min_properties: None,
            max_properties: None,
        }
    }

    /// An open, UNTYPED member host: lists descendants without claiming
    /// the value is an object (falsy scalars skip guarded member reads,
    /// and the conditional truthy⇒object arms carry the strict part).
    pub(crate) fn untyped_member_host() -> Self {
        let mut node = Self::object().with_additional_properties(Self::empty());
        if let Self::Object { typed, .. } = &mut node {
            *typed = false;
        }
        node
    }

    pub(crate) fn closed_object() -> Self {
        Self::object()
            .with_empty_properties()
            .with_additional_properties(Self::foreign(Value::Bool(false)))
    }

    pub(crate) fn unknown_object() -> Self {
        Self::object().with_additional_properties(Self::empty())
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

    pub(crate) fn push_all_of(&mut self, value: SchemaNode) {
        match self {
            Self::Object { all_of, .. } => {
                all_of.push(value);
            }
            Self::Foreign(schema) => {
                push_all_of_value(schema, value);
            }
            _ => {}
        }
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

    fn keyword_schema(key: &str, value: Value) -> Self {
        Self::foreign(Value::Object(
            [(key.to_string(), value)].into_iter().collect(),
        ))
    }

    pub(crate) fn enum_values(values: Vec<Value>) -> Self {
        Self::keyword_schema("enum", Value::Array(values))
    }

    pub(crate) fn const_value(value: Value) -> Self {
        Self::keyword_schema("const", value)
    }

    pub(crate) fn not(value: SchemaNode) -> Self {
        Self::keyword_schema("not", value.into_value())
    }

    pub(crate) fn all_of(values: Vec<SchemaNode>) -> Self {
        match values.len() {
            0 => Self::empty(),
            1 => values.into_iter().next().unwrap_or_else(Self::empty),
            _ => Self::keyword_schema(
                "allOf",
                Value::Array(values.into_iter().map(Self::into_value).collect()),
            ),
        }
    }

    pub(crate) fn any_of(values: Vec<SchemaNode>) -> Self {
        match values.len() {
            0 => Self::empty(),
            1 => values.into_iter().next().unwrap_or_else(Self::empty),
            _ => Self::keyword_schema(
                "anyOf",
                Value::Array(values.into_iter().map(Self::into_value).collect()),
            ),
        }
    }

    pub(crate) fn property_entries(&self) -> Vec<(String, SchemaNode)> {
        match self {
            Self::Object { properties, .. } => properties
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            Self::Foreign(value) => foreign_property_entries(value),
            _ => Vec::new(),
        }
    }

    pub(crate) fn take_property(&mut self, key: &str) -> Option<SchemaNode> {
        match self {
            Self::Object { properties, .. } => properties.remove(key),
            Self::Foreign(Value::Object(object)) => object
                .get_mut("properties")
                .and_then(Value::as_object_mut)
                .and_then(|properties| properties.remove(key))
                .map(SchemaNode::foreign),
            _ => None,
        }
    }

    pub(crate) fn put_property(&mut self, key: String, value: SchemaNode) {
        match self {
            Self::Object { properties, .. } => {
                properties.insert(key, value);
            }
            Self::Foreign(Value::Object(object)) => {
                let properties = object
                    .entry("properties".to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Value::Object(properties) = properties {
                    properties.insert(key, value.into_value());
                }
            }
            _ => {}
        }
    }

    pub(crate) fn is_array_like(&self) -> bool {
        match self {
            Self::Array { .. } => true,
            Self::Object { .. } | Self::Empty | Self::Typed(_) => false,
            Self::Foreign(value) => foreign_is_array_like(value),
        }
    }

    pub(crate) fn is_false_schema(&self) -> bool {
        matches!(self, Self::Foreign(Value::Bool(false)))
    }

    pub(crate) fn opens_unknown_object_fields(&self) -> bool {
        match self {
            Self::Object {
                additional_properties: Some(additional_properties),
                ..
            } => !additional_properties.is_false_schema(),
            Self::Foreign(Value::Object(object)) => foreign_opens_unknown_object_fields(object),
            _ => false,
        }
    }

    pub(crate) fn is_exact_empty_object(&self) -> bool {
        match self {
            Self::Object { max_properties, .. } => *max_properties == Some(0),
            Self::Foreign(Value::Object(object)) => foreign_is_exact_empty_object(object),
            _ => false,
        }
    }

    pub(crate) fn has_object_descendants(&self) -> bool {
        match self {
            Self::Object {
                properties, all_of, ..
            } => !properties.is_empty() || !all_of.is_empty(),
            Self::Foreign(Value::Object(object)) => foreign_has_object_descendants(object),
            _ => false,
        }
    }

    pub(crate) fn is_plain_closed_values_object(&self) -> bool {
        match self {
            Self::Object {
                additional_properties: Some(additional_properties),
                max_properties,
                all_of,
                ..
            } => {
                additional_properties.is_false_schema()
                    && *max_properties != Some(0)
                    && all_of.is_empty()
            }
            Self::Foreign(Value::Object(object)) => foreign_is_plain_closed_values_object(object),
            _ => false,
        }
    }

    pub(crate) fn open_object(&mut self) {
        match self {
            Self::Object {
                additional_properties,
                ..
            } if additional_properties
                .as_deref()
                .is_none_or(Self::is_false_schema) =>
            {
                *additional_properties = Some(Box::new(Self::empty()));
            }
            Self::Foreign(Value::Object(object))
                if object.get("additionalProperties").and_then(Value::as_bool) == Some(false) =>
            {
                object.insert(
                    "additionalProperties".to_string(),
                    Self::empty().into_value(),
                );
            }
            _ => {}
        }
    }

    pub(crate) fn clear_exact_empty_constraint_for_descendant(&mut self) {
        let should_open = match self {
            Self::Object { max_properties, .. } if *max_properties == Some(0) => {
                *max_properties = None;
                true
            }
            Self::Foreign(Value::Object(object)) if foreign_is_exact_empty_object(object) => {
                object.remove("maxProperties");
                true
            }
            _ => false,
        };
        if should_open {
            self.open_object();
        }
    }

    pub(crate) fn path_exists(&self, path_segments: &[String]) -> bool {
        if path_segments.is_empty() {
            return !self.is_empty_slot();
        }

        let Some((head, tail)) = path_segments.split_first() else {
            return false;
        };

        match self {
            Self::Object {
                properties, all_of, ..
            } => {
                if all_of.iter().any(|child| child.path_exists(path_segments)) {
                    return true;
                }
                properties
                    .get(head)
                    .is_some_and(|child| child.path_exists(tail))
            }
            Self::Array { items, .. } if head == "*" => items
                .as_deref()
                .is_some_and(|child| child.path_exists(tail)),
            Self::Typed(schema) => schema.path_exists(path_segments),
            Self::Foreign(value) => foreign_path_exists(value, path_segments),
            _ => false,
        }
    }

    pub(crate) fn is_empty_slot(&self) -> bool {
        match self {
            Self::Empty | Self::Foreign(Value::Null) => true,
            Self::Foreign(value) => crate::schema_model::is_empty_schema(value),
            _ => false,
        }
    }

    pub(crate) fn into_value(self) -> Value {
        match self {
            Self::Empty => Value::Object(Map::new()),
            Self::Object {
                properties,
                typed,
                all_of,
                include_empty_properties,
                required,
                additional_properties,
                min_properties,
                max_properties,
            } => {
                let mut object = if typed {
                    type_map(JsonSchemaType::Object)
                } else {
                    Map::new()
                };
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
                if !all_of.is_empty() {
                    object.insert(
                        "allOf".to_string(),
                        Value::Array(all_of.into_iter().map(Self::into_value).collect()),
                    );
                }
                Value::Object(object)
            }
            Self::Array { items, min_items } => {
                let mut object = type_map(JsonSchemaType::Array);
                // The `Foreign(Null)` placeholder means "no items opinion":
                // an unfilled array slot must not serialize `items: null`,
                // which is not a schema.
                if let Some(items) = items
                    && !matches!(items.as_ref(), Self::Foreign(Value::Null))
                {
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
            Self::Typed(schema) => schema.into_value(),
            Self::Foreign(value) => value,
        }
    }
}

impl TypedSchemaNode {
    fn visit_embedded_values(&self, visit: &mut impl FnMut(&Value)) {
        match self {
            Self::Boolean(_) => {}
            Self::Keywords(keywords) => keywords.visit_embedded_values(visit),
        }
    }

    fn path_exists(&self, path_segments: &[String]) -> bool {
        match self {
            Self::Boolean(_) => false,
            Self::Keywords(keywords) => keywords.path_exists(path_segments),
        }
    }

    fn into_value(self) -> Value {
        match self {
            Self::Boolean(value) => Value::Bool(value),
            Self::Keywords(keywords) => keywords.into_value(),
        }
    }
}

impl SchemaKeywords {
    fn visit_embedded_values(&self, visit: &mut impl FnMut(&Value)) {
        for value in self.extra_keywords.values() {
            visit(value);
        }
        for schema in self.child_schemas() {
            schema.visit_foreign_values(visit);
        }
    }

    fn path_exists(&self, path_segments: &[String]) -> bool {
        let Some((head, tail)) = path_segments.split_first() else {
            return true;
        };
        self.all_of
            .iter()
            .flatten()
            .any(|schema| schema.path_exists(path_segments))
            || self
                .properties
                .as_ref()
                .and_then(|properties| properties.get(head))
                .is_some_and(|schema| schema.path_exists(tail))
            || (head == "*"
                && self
                    .items
                    .as_deref()
                    .is_some_and(|schema| schema.path_exists(tail)))
    }

    fn child_schemas(&self) -> impl Iterator<Item = &SchemaNode> {
        self.properties
            .iter()
            .flat_map(|properties| properties.values())
            .chain(self.additional_properties.iter().map(Box::as_ref))
            .chain(self.items.iter().map(Box::as_ref))
            .chain(self.all_of.iter().flatten())
            .chain(self.any_of.iter().flatten())
            .chain(self.one_of.iter().flatten())
            .chain(self.not.iter().map(Box::as_ref))
            .chain(self.if_schema.iter().map(Box::as_ref))
            .chain(self.then_schema.iter().map(Box::as_ref))
            .chain(self.else_schema.iter().map(Box::as_ref))
    }

    fn into_value(self) -> Value {
        let mut object = self.extra_keywords.into_iter().collect::<Map<_, _>>();
        if let Some(schema_type) = self.schema_type {
            object.insert("type".to_string(), schema_type.into_value());
        }
        if let Some(properties) = self.properties {
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
        if let Some(required) = self.required {
            object.insert(
                "required".to_string(),
                Value::Array(required.into_iter().map(Value::String).collect()),
            );
        }
        insert_schema_keyword(
            &mut object,
            "additionalProperties",
            self.additional_properties,
        );
        insert_schema_keyword(&mut object, "items", self.items);
        insert_schema_array_keyword(&mut object, "allOf", self.all_of);
        insert_schema_array_keyword(&mut object, "anyOf", self.any_of);
        insert_schema_array_keyword(&mut object, "oneOf", self.one_of);
        insert_schema_keyword(&mut object, "not", self.not);
        insert_schema_keyword(&mut object, "if", self.if_schema);
        insert_schema_keyword(&mut object, "then", self.then_schema);
        insert_schema_keyword(&mut object, "else", self.else_schema);
        insert_u64_keyword(&mut object, "minProperties", self.min_properties);
        insert_u64_keyword(&mut object, "maxProperties", self.max_properties);
        insert_u64_keyword(&mut object, "minItems", self.min_items);
        Value::Object(object)
    }
}

impl SchemaTypeKeyword {
    fn into_value(self) -> Value {
        match self {
            Self::Single(schema_type) => Value::String(schema_type.as_str().to_string()),
            Self::Multiple(schema_types) => Value::Array(
                schema_types
                    .into_iter()
                    .map(|schema_type| Value::String(schema_type.as_str().to_string()))
                    .collect(),
            ),
        }
    }
}

fn take_schema_type(object: &mut Map<String, Value>) -> Option<SchemaTypeKeyword> {
    let parsed = match object.get("type")? {
        Value::String(name) => JsonSchemaType::from_name(name).map(SchemaTypeKeyword::Single),
        Value::Array(names) => names
            .iter()
            .map(|name| JsonSchemaType::from_name(name.as_str()?))
            .collect::<Option<Vec<_>>>()
            .map(SchemaTypeKeyword::Multiple),
        _ => None,
    }?;
    object.remove("type");
    Some(parsed)
}

fn take_object_keyword(object: &mut Map<String, Value>, key: &str) -> Option<Map<String, Value>> {
    match object.remove(key)? {
        Value::Object(value) => Some(value),
        value => {
            object.insert(key.to_string(), value);
            None
        }
    }
}

fn take_string_array_keyword(object: &mut Map<String, Value>, key: &str) -> Option<Vec<String>> {
    let values = object
        .get(key)?
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    object.remove(key);
    Some(values)
}

fn take_schema_keyword(object: &mut Map<String, Value>, key: &str) -> Option<Box<SchemaNode>> {
    object.remove(key).map(SchemaNode::from_value).map(Box::new)
}

fn take_schema_array_keyword(
    object: &mut Map<String, Value>,
    key: &str,
) -> Option<Vec<SchemaNode>> {
    object.get(key)?.as_array()?;
    let Value::Array(values) = object.remove(key)? else {
        return None;
    };
    Some(values.into_iter().map(SchemaNode::from_value).collect())
}

fn take_u64_keyword(object: &mut Map<String, Value>, key: &str) -> Option<u64> {
    let value = object.get(key)?.as_u64()?;
    object.remove(key);
    Some(value)
}

fn insert_schema_keyword(
    object: &mut Map<String, Value>,
    key: &str,
    schema: Option<Box<SchemaNode>>,
) {
    if let Some(schema) = schema {
        object.insert(key.to_string(), schema.into_value());
    }
}

fn insert_schema_array_keyword(
    object: &mut Map<String, Value>,
    key: &str,
    schemas: Option<Vec<SchemaNode>>,
) {
    if let Some(schemas) = schemas {
        object.insert(
            key.to_string(),
            Value::Array(schemas.into_iter().map(SchemaNode::into_value).collect()),
        );
    }
}

fn insert_u64_keyword(object: &mut Map<String, Value>, key: &str, value: Option<u64>) {
    if let Some(value) = value {
        object.insert(key.to_string(), Value::Number(Number::from(value)));
    }
}

fn push_all_of_value(schema: &mut Value, value: SchemaNode) {
    let Value::Object(object) = schema else {
        return;
    };
    let all_of = object
        .entry("allOf".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Value::Array(entries) = all_of {
        entries.push(value.into_value());
    }
}

fn foreign_property_entries(value: &Value) -> Vec<(String, SchemaNode)> {
    value
        .as_object()
        .and_then(|object| object.get("properties"))
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .map(|(key, value)| (key.clone(), SchemaNode::foreign(value.clone())))
                .collect()
        })
        .unwrap_or_default()
}

fn foreign_is_array_like(value: &Value) -> bool {
    match crate::schema_model::schema_type(value) {
        Some("array") => true,
        Some(_) => false,
        None => value
            .as_object()
            .is_some_and(|object| object.contains_key("items")),
    }
}

fn foreign_opens_unknown_object_fields(object: &Map<String, Value>) -> bool {
    object
        .get("additionalProperties")
        .is_some_and(|value| value.as_bool() != Some(false))
        || object
            .get("x-kubernetes-preserve-unknown-fields")
            .and_then(Value::as_bool)
            == Some(true)
}

fn foreign_is_exact_empty_object(object: &Map<String, Value>) -> bool {
    object.get("maxProperties").and_then(Value::as_u64) == Some(0)
}

fn foreign_has_object_descendants(object: &Map<String, Value>) -> bool {
    object
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|properties| !properties.is_empty())
        || object
            .get("allOf")
            .and_then(Value::as_array)
            .is_some_and(|entries| !entries.is_empty())
}

fn foreign_is_plain_closed_values_object(object: &Map<String, Value>) -> bool {
    object.get("type").and_then(Value::as_str) == Some("object")
        && object.get("additionalProperties").and_then(Value::as_bool) == Some(false)
        && object
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|properties| !properties.is_empty())
        && !object.contains_key("anyOf")
        && !object.contains_key("oneOf")
        && !object.contains_key("allOf")
        && !object.contains_key("description")
        && !object
            .keys()
            .any(|key| key.starts_with("x-kubernetes-") || key == "$ref")
}

fn foreign_path_exists(value: &Value, path_segments: &[String]) -> bool {
    if path_segments.is_empty() {
        return !crate::schema_model::is_empty_schema(value);
    }

    let Some((head, tail)) = path_segments.split_first() else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };

    for composition_key in ["anyOf", "allOf", "oneOf"] {
        if let Some(entries) = object.get(composition_key).and_then(Value::as_array)
            && entries
                .iter()
                .any(|entry| foreign_path_exists(entry, path_segments))
        {
            return true;
        }
    }

    if head == "*" {
        return object
            .get("items")
            .is_some_and(|child| foreign_path_exists(child, tail));
    }

    object
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get(head))
        .is_some_and(|child| foreign_path_exists(child, tail))
}

fn type_map(ty: JsonSchemaType) -> Map<String, Value> {
    Map::from_iter([("type".to_string(), Value::String(ty.as_str().to_string()))])
}
