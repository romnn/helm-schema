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
    /// Every alternative matches; an empty list matches every JSON value.
    AllOf(Vec<Self>),
    /// An array has at least one matching item; an empty array does not match.
    Contains(Box<Self>),
    /// At least one alternative matches; an empty list matches nothing.
    AnyOf(Vec<Self>),
}

impl ValuePattern {
    /// Tests JSON membership, returning `None` when an invalid pattern prevents a verdict.
    #[must_use]
    pub fn matches(&self, value: &Value) -> Option<bool> {
        match self {
            Self::Any => Some(true),
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
