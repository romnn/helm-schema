use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Number;

/// Scalar literal used by values-decidable guard comparisons.
///
/// Helm `eq` / `ne` conditions can compare against strings, booleans, numbers,
/// and nil. Keeping the literal typed prevents static analysis from degrading
/// `eq .Values.enabled false` into a misleading truthiness guard.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuardValue {
    String(String),
    Bool(bool),
    Int(i64),
    Float(String),
    Null,
}

impl GuardValue {
    #[must_use]
    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }

    #[must_use]
    pub fn float(value: f64) -> Option<Self> {
        value.is_finite().then(|| Self::Float(value.to_string()))
    }
}

impl Serialize for GuardValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::String(value) => serializer.serialize_str(value),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Int(value) => serializer.serialize_i64(*value),
            Self::Float(value) => {
                let number = value
                    .parse::<f64>()
                    .ok()
                    .and_then(Number::from_f64)
                    .ok_or_else(|| serde::ser::Error::custom("invalid float guard value"))?;
                number.serialize(serializer)
            }
            Self::Null => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for GuardValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(value) => Ok(Self::String(value)),
            serde_json::Value::Bool(value) => Ok(Self::Bool(value)),
            serde_json::Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    Ok(Self::Int(value))
                } else {
                    Ok(Self::Float(value.to_string()))
                }
            }
            serde_json::Value::Null => Ok(Self::Null),
            _ => Err(serde::de::Error::custom(
                "guard comparison value must be a scalar literal",
            )),
        }
    }
}

impl fmt::Display for GuardValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(value) => value.fmt(f),
            Self::Bool(value) => value.fmt(f),
            Self::Int(value) => value.fmt(f),
            Self::Float(value) => value.fmt(f),
            Self::Null => f.write_str("null"),
        }
    }
}

/// A guard condition from an `if`, `with`, or `range` block.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Guard {
    /// Simple truthy check: `if .Values.X`
    Truthy { path: String },
    /// Negated truthy check: `if not .Values.X`
    Not { path: String },
    /// Equality check: `if eq .Values.X "value"` / `if eq .Values.X false`.
    Eq { path: String, value: GuardValue },
    /// Inequality check: `if ne .Values.X "value"` / `if ne .Values.X false`.
    NotEq { path: String, value: GuardValue },
    /// Path absence check, used for structural rules where missing values are
    /// semantically distinct from false values.
    Absent { path: String },
    /// The path's string value matches a literal regular expression:
    /// `if regexMatch "…" .Values.X`. `regexMatch` type-asserts a string
    /// subject, so the guard holding implies string-ness as well. When
    /// `templated` is set the subject reached the match through `tpl`, so
    /// the pattern constrains the rendered OUTPUT: a raw value carrying a
    /// template action is admitted regardless (its render may match).
    MatchesPattern {
        path: String,
        pattern: String,
        templated: bool,
    },
    /// Disjunction: `if or .Values.A .Values.B`
    Or { paths: Vec<String> },
    /// Disjunction whose arms may each contain a conjunction of typed guards.
    ///
    /// This preserves structural forms such as
    /// `or (and .Values.A .Values.B) (eq .Values.mode "prod")` without
    /// degrading them into truthiness checks for every mentioned path.
    AnyOf { alternatives: Vec<Vec<Guard>> },
    /// Body of `range .Values.X` / `range .foo` block. The referenced path is
    /// being iterated as a collection, not interpreted as a boolean-valued
    /// scalar. This should not contribute a boolean type hint downstream.
    Range { path: String },
    /// Body of `with .Values.X` block. This distinguishes header binding from
    /// `if`-style truthy checks. The bound path is null-tolerant by
    /// construction because `with nil` skips the body.
    With { path: String },
    /// Rendered via a `default ... <path>` fallback, either in prefix form
    /// (`default "x" .Values.X`) or pipeline form (`.Values.X | default "x"`).
    ///
    /// This is stronger than a plain truthy guard: the template explicitly
    /// substitutes a fallback when the path is empty/nil, so `null` is an
    /// accepted chart input for that render site even when `values.yaml` ships
    /// a non-null default.
    Default { path: String },
    /// A `typeIs "<json type>" <path>` check in template logic.
    ///
    /// This is not a truthiness guard. It is a structural type declaration:
    /// helpers such as Bitnami's `common.tplvalues.render` explicitly branch on
    /// `typeIs "string" .value`, so callers may supply that values path as a
    /// string even when another branch renders it as a YAML object fragment.
    TypeIs { path: String, schema_type: String },
    /// The complement of [`Guard::TypeIs`]: the `else` arm of a type
    /// dispatch (`if typeIs "string" x … else …`).
    ///
    /// Rows need this as a first-class variant because dropping the
    /// complement collapses a type-switch partition: member reads and
    /// structural placements under the `else` would otherwise apply to
    /// EVERY type of the dispatched path.
    NotTypeIs { path: String, schema_type: String },
}

impl Guard {
    pub(crate) fn canonicalize_all(guards: &mut Vec<Self>) {
        for guard in guards.iter_mut() {
            guard.canonicalize();
        }
        guards.sort();
        guards.dedup();
    }

    fn canonicalize(&mut self) {
        match self {
            Self::Or { paths } => {
                paths.sort();
                paths.dedup();
            }
            Self::AnyOf { alternatives } => {
                for guards in alternatives.iter_mut() {
                    Self::canonicalize_all(guards);
                }
                alternatives.sort();
                alternatives.dedup();
            }
            Self::Truthy { .. }
            | Self::Not { .. }
            | Self::Eq { .. }
            | Self::NotEq { .. }
            | Self::Absent { .. }
            | Self::MatchesPattern { .. }
            | Self::Range { .. }
            | Self::With { .. }
            | Self::Default { .. }
            | Self::TypeIs { .. }
            | Self::NotTypeIs { .. } => {}
        }
    }

    /// Return all `.Values.*` paths referenced by this guard.
    #[must_use]
    pub fn value_paths(&self) -> Vec<&str> {
        match self {
            Guard::Truthy { path }
            | Guard::Not { path }
            | Guard::Eq { path, .. }
            | Guard::NotEq { path, .. }
            | Guard::Absent { path }
            | Guard::MatchesPattern { path, .. }
            | Guard::Range { path }
            | Guard::With { path }
            | Guard::Default { path }
            | Guard::TypeIs { path, .. }
            | Guard::NotTypeIs { path, .. } => {
                vec![path.as_str()]
            }
            Guard::Or { paths } => paths.iter().map(std::string::String::as_str).collect(),
            Guard::AnyOf { alternatives } => alternatives
                .iter()
                .flat_map(|alternative| alternative.iter().flat_map(Guard::value_paths))
                .collect(),
        }
    }

    /// Rewrite value paths carried by this guard.
    #[must_use]
    pub fn map_value_paths<F>(self, map: &mut F) -> Self
    where
        F: FnMut(&str) -> String,
    {
        match self {
            Guard::Truthy { path } => Guard::Truthy { path: map(&path) },
            Guard::Not { path } => Guard::Not { path: map(&path) },
            Guard::Eq { path, value } => Guard::Eq {
                path: map(&path),
                value,
            },
            Guard::NotEq { path, value } => Guard::NotEq {
                path: map(&path),
                value,
            },
            Guard::Absent { path } => Guard::Absent { path: map(&path) },
            Guard::MatchesPattern {
                path,
                pattern,
                templated,
            } => Guard::MatchesPattern {
                path: map(&path),
                pattern,
                templated,
            },
            Guard::Or { paths } => Guard::Or {
                paths: paths.into_iter().map(|path| map(&path)).collect(),
            },
            Guard::AnyOf { alternatives } => Guard::AnyOf {
                alternatives: alternatives
                    .into_iter()
                    .map(|alternative| {
                        alternative
                            .into_iter()
                            .map(|guard| guard.map_value_paths(map))
                            .collect()
                    })
                    .collect(),
            },
            Guard::Range { path } => Guard::Range { path: map(&path) },
            Guard::With { path } => Guard::With { path: map(&path) },
            Guard::Default { path } => Guard::Default { path: map(&path) },
            Guard::TypeIs { path, schema_type } => Guard::TypeIs {
                path: map(&path),
                schema_type,
            },
            Guard::NotTypeIs { path, schema_type } => Guard::NotTypeIs {
                path: map(&path),
                schema_type,
            },
        }
    }
}
