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
    pub(crate) schema_type: Option<SchemaTypeKeyword>,
    pub(crate) reference: Option<String>,
    pub(crate) pattern: Option<String>,
    pub(crate) properties: Option<BTreeMap<String, SchemaNode>>,
    pub(crate) required: Option<Vec<String>>,
    pub(crate) additional_properties: Option<Box<SchemaNode>>,
    pub(crate) items: Option<Box<SchemaNode>>,
    pub(crate) all_of: Option<Vec<SchemaNode>>,
    pub(crate) any_of: Option<Vec<SchemaNode>>,
    pub(crate) one_of: Option<Vec<SchemaNode>>,
    pub(crate) not: Option<Box<SchemaNode>>,
    pub(crate) if_schema: Option<Box<SchemaNode>>,
    pub(crate) then_schema: Option<Box<SchemaNode>>,
    pub(crate) else_schema: Option<Box<SchemaNode>>,
    pub(crate) min_properties: Option<u64>,
    pub(crate) max_properties: Option<u64>,
    pub(crate) min_items: Option<u64>,
    pub(crate) extra_keywords: BTreeMap<String, Value>,
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
            Self::Typed(schema) => visit(&schema.clone().into_value()),
            Self::Foreign(value) => visit(value),
        }
    }

    pub(crate) fn empty() -> Self {
        Self::Empty
    }

    pub(crate) fn foreign(value: Value) -> Self {
        Self::from_value(value)
    }

    pub(crate) fn from_value(value: Value) -> Self {
        match value {
            Value::Bool(value) => Self::Typed(TypedSchemaNode::Boolean(value)),
            Value::Object(mut object) => {
                let schema_type = take_schema_type(&mut object);
                let reference = take_string_keyword(&mut object, "$ref");
                let pattern = take_string_keyword(&mut object, "pattern");
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
                    reference,
                    pattern,
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
        Self::Typed(TypedSchemaNode::Keywords(Box::new(SchemaKeywords {
            schema_type: Some(SchemaTypeKeyword::Single(ty)),
            ..SchemaKeywords::default()
        })))
    }

    pub(crate) fn type_named(name: &str) -> Self {
        Self::keyword_schema("type", Value::String(name.to_string()))
    }

    pub(crate) fn reference(reference: impl Into<String>) -> Self {
        Self::keyword_schema("$ref", Value::String(reference.into()))
    }

    pub(crate) fn typed_keyword(mut self, key: impl Into<String>, value: Value) -> Self {
        let key = key.into();
        match &mut self {
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => {
                match (key.as_str(), value) {
                    ("$ref", Value::String(reference)) => keywords.reference = Some(reference),
                    ("pattern", Value::String(pattern)) => keywords.pattern = Some(pattern),
                    (_, value) => {
                        keywords.extra_keywords.insert(key, value);
                    }
                }
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
        match &mut self {
            Self::Object { properties, .. } => {
                properties.insert(key.into(), value);
            }
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => {
                keywords
                    .properties
                    .get_or_insert_with(BTreeMap::new)
                    .insert(key.into(), value);
            }
            _ => {}
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
        let key = key.into();
        match &mut self {
            Self::Object { required, .. } => {
                required.insert(key);
            }
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => {
                let required = keywords.required.get_or_insert_with(Vec::new);
                if !required.contains(&key) {
                    required.push(key);
                    required.sort();
                }
            }
            _ => {}
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
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => {
                keywords.all_of.get_or_insert_with(Vec::new).push(value);
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
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => keywords
                .properties
                .iter()
                .flat_map(|properties| properties.iter())
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn take_property(&mut self, key: &str) -> Option<SchemaNode> {
        match self {
            Self::Object { properties, .. } => properties.remove(key),
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => keywords
                .properties
                .as_mut()
                .and_then(|properties| properties.remove(key)),
            _ => None,
        }
    }

    pub(crate) fn put_property(&mut self, key: String, value: SchemaNode) {
        match self {
            Self::Object { properties, .. } => {
                properties.insert(key, value);
            }
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => {
                keywords
                    .properties
                    .get_or_insert_with(BTreeMap::new)
                    .insert(key, value);
            }
            _ => {}
        }
    }

    pub(crate) fn is_array_like(&self) -> bool {
        match self {
            Self::Array { .. } => true,
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => keywords.is_array_like(),
            Self::Object { .. }
            | Self::Empty
            | Self::Typed(TypedSchemaNode::Boolean(_))
            | Self::Foreign(_) => false,
        }
    }

    pub(crate) fn is_false_schema(&self) -> bool {
        matches!(self, Self::Typed(TypedSchemaNode::Boolean(false)))
    }

    pub(crate) fn opens_unknown_object_fields(&self) -> bool {
        match self {
            Self::Object {
                additional_properties: Some(additional_properties),
                ..
            } => !additional_properties.is_false_schema(),
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => {
                keywords.opens_unknown_object_fields()
            }
            _ => false,
        }
    }

    pub(crate) fn make_explicitly_open_object(&mut self) {
        match self {
            Self::Object {
                additional_properties,
                ..
            } if additional_properties.is_none() => {
                *additional_properties = Some(Box::new(Self::empty()));
            }
            Self::Typed(TypedSchemaNode::Keywords(keywords))
                if keywords.schema_type
                    == Some(SchemaTypeKeyword::Single(JsonSchemaType::Object))
                    && keywords.additional_properties.is_none() =>
            {
                keywords.additional_properties = Some(Box::new(Self::empty()));
            }
            _ => {}
        }
    }

    pub(crate) fn has_negated_pattern(&self, pattern: &str) -> bool {
        match self {
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => {
                keywords.has_negated_pattern(pattern)
            }
            _ => false,
        }
    }

    pub(crate) fn references(&self, reference: &str) -> bool {
        match self {
            Self::Object {
                properties,
                all_of,
                additional_properties,
                ..
            } => {
                properties
                    .values()
                    .any(|schema| schema.references(reference))
                    || all_of.iter().any(|schema| schema.references(reference))
                    || additional_properties
                        .as_deref()
                        .is_some_and(|schema| schema.references(reference))
            }
            Self::Array { items, .. } => items
                .as_deref()
                .is_some_and(|schema| schema.references(reference)),
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => keywords.references(reference),
            Self::Empty | Self::Typed(TypedSchemaNode::Boolean(_)) | Self::Foreign(_) => false,
        }
    }

    pub(crate) fn is_not_reference(&self, reference: &str) -> bool {
        matches!(
            self,
            Self::Typed(TypedSchemaNode::Keywords(keywords))
                if keywords.not.as_deref().is_some_and(|schema| {
                    matches!(
                        schema,
                        Self::Typed(TypedSchemaNode::Keywords(not_keywords))
                            if not_keywords.reference.as_deref() == Some(reference)
                    )
                })
        )
    }

    pub(crate) fn is_exact_empty_object(&self) -> bool {
        match self {
            Self::Object { max_properties, .. } => *max_properties == Some(0),
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => keywords.max_properties == Some(0),
            _ => false,
        }
    }

    pub(crate) fn has_object_descendants(&self) -> bool {
        match self {
            Self::Object {
                properties, all_of, ..
            } => !properties.is_empty() || !all_of.is_empty(),
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => keywords.has_object_descendants(),
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
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => {
                keywords.is_plain_closed_values_object()
            }
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
            Self::Typed(TypedSchemaNode::Keywords(keywords))
                if keywords
                    .additional_properties
                    .as_deref()
                    .is_some_and(Self::is_false_schema) =>
            {
                keywords.additional_properties = Some(Box::new(Self::empty()));
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
            Self::Typed(TypedSchemaNode::Keywords(keywords))
                if keywords.max_properties == Some(0) =>
            {
                keywords.max_properties = None;
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
            _ => false,
        }
    }

    pub(crate) fn is_empty_slot(&self) -> bool {
        match self {
            Self::Empty | Self::Foreign(Value::Null) => true,
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => keywords.is_empty_schema(),
            _ => false,
        }
    }

    pub(crate) fn into_parsed_representation(self) -> Self {
        match self {
            Self::Empty => Self::Typed(TypedSchemaNode::Keywords(Box::default())),
            Self::Object {
                properties,
                typed,
                all_of,
                include_empty_properties,
                required,
                additional_properties,
                min_properties,
                max_properties,
            } => Self::Typed(TypedSchemaNode::Keywords(Box::new(SchemaKeywords {
                schema_type: typed.then_some(SchemaTypeKeyword::Single(JsonSchemaType::Object)),
                properties: (include_empty_properties || !properties.is_empty()).then(|| {
                    properties
                        .into_iter()
                        .map(|(key, value)| (key, value.into_parsed_representation()))
                        .collect()
                }),
                required: (!required.is_empty()).then(|| required.into_iter().collect()),
                additional_properties: additional_properties
                    .map(|value| Box::new(value.into_parsed_representation())),
                all_of: (!all_of.is_empty()).then(|| {
                    all_of
                        .into_iter()
                        .map(Self::into_parsed_representation)
                        .collect()
                }),
                min_properties,
                max_properties,
                ..SchemaKeywords::default()
            }))),
            Self::Array { items, min_items } => {
                let items = items
                    .filter(|items| !matches!(items.as_ref(), Self::Foreign(Value::Null)))
                    .map(|items| Box::new(items.into_parsed_representation()));
                Self::Typed(TypedSchemaNode::Keywords(Box::new(SchemaKeywords {
                    schema_type: Some(SchemaTypeKeyword::Single(JsonSchemaType::Array)),
                    items,
                    min_items,
                    ..SchemaKeywords::default()
                })))
            }
            Self::Typed(TypedSchemaNode::Keywords(keywords)) => Self::Typed(
                TypedSchemaNode::Keywords(Box::new(keywords.into_parsed_representation())),
            ),
            Self::Typed(TypedSchemaNode::Boolean(value)) => {
                Self::Typed(TypedSchemaNode::Boolean(value))
            }
            Self::Foreign(value) => Self::from_value(value),
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
    fn into_parsed_representation(mut self) -> Self {
        if let Some(properties) = &mut self.properties {
            for schema in properties.values_mut() {
                let value = std::mem::replace(schema, SchemaNode::Empty);
                *schema = value.into_parsed_representation();
            }
        }
        for schema in [
            &mut self.additional_properties,
            &mut self.items,
            &mut self.not,
            &mut self.if_schema,
            &mut self.then_schema,
            &mut self.else_schema,
        ]
        .into_iter()
        .flatten()
        {
            let value = std::mem::replace(schema.as_mut(), SchemaNode::Empty);
            **schema = value.into_parsed_representation();
        }
        for schemas in [&mut self.all_of, &mut self.any_of, &mut self.one_of]
            .into_iter()
            .flatten()
        {
            for schema in schemas {
                let value = std::mem::replace(schema, SchemaNode::Empty);
                *schema = value.into_parsed_representation();
            }
        }
        self
    }

    pub(crate) fn is_array_like(&self) -> bool {
        match &self.schema_type {
            Some(SchemaTypeKeyword::Single(JsonSchemaType::Array)) => true,
            Some(SchemaTypeKeyword::Single(_) | SchemaTypeKeyword::Multiple(_)) => false,
            None => self.items.is_some(),
        }
    }

    fn opens_unknown_object_fields(&self) -> bool {
        self.additional_properties
            .as_deref()
            .is_some_and(|schema| !schema.is_false_schema())
            || self
                .extra_keywords
                .get("x-kubernetes-preserve-unknown-fields")
                .and_then(Value::as_bool)
                == Some(true)
    }

    fn has_object_descendants(&self) -> bool {
        self.properties
            .as_ref()
            .is_some_and(|properties| !properties.is_empty())
            || self
                .all_of
                .as_ref()
                .is_some_and(|schemas| !schemas.is_empty())
    }

    fn is_plain_closed_values_object(&self) -> bool {
        self.schema_type == Some(SchemaTypeKeyword::Single(JsonSchemaType::Object))
            && self
                .additional_properties
                .as_deref()
                .is_some_and(SchemaNode::is_false_schema)
            && self.max_properties != Some(0)
            && self
                .properties
                .as_ref()
                .is_some_and(|properties| !properties.is_empty())
            && self.all_of.is_none()
            && self.any_of.is_none()
            && self.one_of.is_none()
            && !self.extra_keywords.contains_key("description")
            && self.reference.is_none()
            && !self
                .extra_keywords
                .keys()
                .any(|key| key.starts_with("x-kubernetes-"))
    }

    pub(crate) fn is_empty_schema(&self) -> bool {
        self == &Self::default()
    }

    fn has_negated_pattern(&self, pattern: &str) -> bool {
        self.not.as_deref().is_some_and(|schema| {
            matches!(
                schema,
                SchemaNode::Typed(TypedSchemaNode::Keywords(keywords))
                    if keywords.pattern.as_deref() == Some(pattern)
            )
        }) || [&self.all_of, &self.any_of, &self.one_of]
            .into_iter()
            .flatten()
            .flatten()
            .any(|schema| schema.has_negated_pattern(pattern))
    }

    fn references(&self, reference: &str) -> bool {
        self.reference.as_deref() == Some(reference)
            || self.properties.as_ref().is_some_and(|properties| {
                properties
                    .values()
                    .any(|schema| schema.references(reference))
            })
            || self
                .additional_properties
                .as_deref()
                .is_some_and(|schema| schema.references(reference))
            || self
                .items
                .as_deref()
                .is_some_and(|schema| schema.references(reference))
            || [&self.all_of, &self.any_of, &self.one_of]
                .into_iter()
                .flatten()
                .flatten()
                .any(|schema| schema.references(reference))
            || [
                &self.not,
                &self.if_schema,
                &self.then_schema,
                &self.else_schema,
            ]
            .into_iter()
            .flatten()
            .any(|schema| schema.references(reference))
    }

    fn path_exists(&self, path_segments: &[String]) -> bool {
        let Some((head, tail)) = path_segments.split_first() else {
            return true;
        };
        [&self.any_of, &self.all_of, &self.one_of]
            .into_iter()
            .flatten()
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

    fn into_value(self) -> Value {
        let mut object = self.extra_keywords.into_iter().collect::<Map<_, _>>();
        if let Some(schema_type) = self.schema_type {
            object.insert("type".to_string(), schema_type.into_value());
        }
        insert_string_keyword(&mut object, "$ref", self.reference);
        insert_string_keyword(&mut object, "pattern", self.pattern);
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

fn take_string_keyword(object: &mut Map<String, Value>, key: &str) -> Option<String> {
    let value = object.get(key)?.as_str()?.to_string();
    object.remove(key);
    Some(value)
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

fn insert_string_keyword(object: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        object.insert(key.to_string(), Value::String(value));
    }
}

fn type_map(ty: JsonSchemaType) -> Map<String, Value> {
    Map::from_iter([("type".to_string(), Value::String(ty.as_str().to_string()))])
}
