//! Typed structural membership used by values-decidable output predicates.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::CollectionQuantifier;

/// A bounded structural pattern over one JSON value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuePattern {
    /// Every JSON value.
    Any,
    /// A null, Boolean, or number.
    Scalar,
    /// No string or mapping key contains a template action opener or Helm's removed marker.
    TemplateFree,
    /// No string or mapping key contains a template action opener.
    TemplateActionFree,
    /// No string or mapping key contains Helm's removed marker.
    MarkerFree,
    /// Every string and mapping key has a spelling stable in a YAML flow node.
    YamlFlowStable,
    /// A nullable map of nullable strings, as decoded by Helm's metadata reader.
    StringMap,
    /// A string matching the regular expression.
    String {
        /// Regular expression tested against the string.
        pattern: String,
    },
    /// An array containing every required prefix item in order.
    Array {
        /// Required items, beginning at index zero.
        prefix: Vec<Self>,
        /// Pattern for remaining items; `None` forbids additional items.
        items: Option<Box<Self>>,
    },
    /// An object whose property names and values match their respective patterns.
    Object {
        /// Pattern applied to every property name as a string.
        keys: Box<Self>,
        /// Pattern applied to every property value.
        values: Box<Self>,
    },
    /// The enclosed pattern does not match.
    Not(Box<Self>),
    /// Every alternative matches; an empty list matches every JSON value.
    AllOf(Vec<Self>),
    /// An array has at least one matching item; an empty array does not match.
    Contains(Box<Self>),
    /// At least one alternative matches; an empty list matches nothing.
    AnyOf(Vec<Self>),
}

impl ValuePattern {
    /// Characters that withdraw a proved YAML flow spelling.
    pub const YAML_FLOW_UNSTABLE_STRING_PATTERN: &str = "[^A-Za-z0-9_ ./-]";

    /// Tests JSON membership, returning `None` when an invalid pattern prevents a verdict.
    #[must_use]
    pub fn matches(&self, value: &Value) -> Option<bool> {
        match self {
            Self::Any => Some(true),
            Self::Scalar => Some(matches!(
                value,
                Value::Null | Value::Bool(_) | Value::Number(_)
            )),
            Self::TemplateFree => Some(serialized_text_unchanged(value, true)),
            Self::TemplateActionFree => {
                Some(serialized_texts_match(value, |text| !text.contains("{{")))
            }
            Self::MarkerFree => Some(serialized_text_unchanged(value, false)),
            Self::YamlFlowStable => {
                static PATTERN: std::sync::LazyLock<Result<regex::Regex, regex::Error>> =
                    std::sync::LazyLock::new(|| {
                        regex::Regex::new(ValuePattern::YAML_FLOW_UNSTABLE_STRING_PATTERN)
                    });
                let pattern = PATTERN.as_ref().ok()?;
                Some(serialized_texts_match(value, |text| {
                    !pattern.is_match(text)
                }))
            }
            Self::StringMap => Some(match value {
                Value::Null => true,
                Value::Object(fields) => fields
                    .values()
                    .all(|value| value.is_string() || value.is_null()),
                Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => false,
            }),
            Self::String { pattern } => match value {
                Value::String(value) => regex::Regex::new(pattern)
                    .ok()
                    .map(|pattern| pattern.is_match(value)),
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::Array(_)
                | Value::Object(_) => Some(false),
            },
            Self::Array { prefix, items } => match value {
                Value::Array(values) => {
                    if values.len() < prefix.len()
                        || (items.is_none() && values.len() != prefix.len())
                    {
                        return Some(false);
                    }
                    quantify(
                        CollectionQuantifier::All,
                        values.iter().enumerate().map(|(index, value)| {
                            prefix.get(index).or(items.as_deref())?.matches(value)
                        }),
                    )
                }
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Object(_) => Some(false),
            },
            Self::Object { keys, values } => match value {
                Value::Object(fields) => quantify(
                    CollectionQuantifier::All,
                    fields.iter().flat_map(|(key, value)| {
                        [
                            keys.matches(&Value::String(key.clone())),
                            values.matches(value),
                        ]
                    }),
                ),
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Array(_) => Some(false),
            },
            Self::Not(pattern) => pattern.matches(value).map(|matched| !matched),
            Self::AllOf(patterns) => quantify(
                CollectionQuantifier::All,
                patterns.iter().map(|pattern| pattern.matches(value)),
            ),
            Self::Contains(pattern) => match value {
                Value::Array(values) => quantify(
                    CollectionQuantifier::Any,
                    values.iter().map(|value| pattern.matches(value)),
                ),
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Object(_) => Some(false),
            },
            Self::AnyOf(patterns) => quantify(
                CollectionQuantifier::Any,
                patterns.iter().map(|pattern| pattern.matches(value)),
            ),
        }
    }
}

fn quantify(
    quantifier: CollectionQuantifier,
    matches: impl Iterator<Item = Option<bool>>,
) -> Option<bool> {
    let decisive = match quantifier {
        CollectionQuantifier::All => false,
        CollectionQuantifier::Any => true,
    };
    let mut unknown = false;
    for result in matches {
        match result {
            Some(value) => {
                if value == decisive {
                    return Some(value);
                }
            }
            None => unknown = true,
        }
    }
    (!unknown).then_some(!decisive)
}

fn serialized_text_unchanged(value: &Value, templated: bool) -> bool {
    serialized_texts_match(value, |text| {
        !text.contains("<no value>") && (!templated || !text.contains("{{"))
    })
}

fn serialized_texts_match(value: &Value, preserved: impl Fn(&str) -> bool) -> bool {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::String(text) if !preserved(text) => return false,
            Value::Array(items) => pending.extend(items),
            Value::Object(fields) => {
                if fields.keys().any(|key| !preserved(key)) {
                    return false;
                }
                pending.extend(fields.values());
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }
    true
}
