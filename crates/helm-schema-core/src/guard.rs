use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Number;

use crate::ValuesPath;

/// Scalar literal used by values-decidable guard comparisons.
///
/// Helm `eq` / `ne` conditions can compare against strings, booleans, numbers,
/// and nil. Keeping the literal typed prevents static analysis from degrading
/// `eq .Values.enabled false` into a misleading truthiness guard.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuardValue {
    /// UTF-8 string literal.
    String(String),
    /// Boolean literal.
    Bool(bool),
    /// Signed integer literal.
    Int(i64),
    /// Finite floating-point literal stored in canonical textual form.
    Float(String),
    /// Explicit null literal.
    Null,
}

impl GuardValue {
    /// Creates a string guard literal.
    #[must_use]
    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }

    /// Creates a finite floating-point guard literal, rejecting NaN and infinity.
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
            #[expect(
                clippy::match_same_arms,
                reason = "string and float variants require distinct bindings despite identical formatting"
            )]
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
    Truthy {
        /// Values path tested for truthiness.
        path: ValuesPath,
    },
    /// Negated truthy check: `if not .Values.X`
    Not {
        /// Values path tested for falsiness.
        path: ValuesPath,
    },
    /// Equality check: `if eq .Values.X "value"` / `if eq .Values.X false`.
    Eq {
        /// Values path compared with the literal.
        path: ValuesPath,
        /// Literal required at the path.
        value: GuardValue,
    },
    /// Inequality check: `if ne .Values.X "value"` / `if ne .Values.X false`.
    NotEq {
        /// Values path compared with the literal.
        path: ValuesPath,
        /// Literal excluded at the path.
        value: GuardValue,
    },
    /// Path absence check, used for structural rules where missing values are
    /// semantically distinct from false values.
    Absent {
        /// Values path whose absence selects the branch.
        path: ValuesPath,
    },
    /// The path's string value matches a literal regular expression:
    /// `if regexMatch "…" .Values.X`. `regexMatch` type-asserts a string
    /// subject, so the guard holding implies string-ness as well. When
    /// `templated` is set the subject reached the match through `tpl`, so
    /// the pattern constrains the rendered OUTPUT: a raw value carrying a
    /// template action is admitted regardless (its render may match).
    MatchesPattern {
        /// Values path subjected to the pattern test.
        path: ValuesPath,
        /// Literal regular expression required by the branch.
        pattern: String,
        /// Whether matching occurs after rendering the value through `tpl`.
        templated: bool,
    },
    /// The path is a string that does not match a literal regular expression.
    ///
    /// This is narrower than the logical complement of
    /// [`Guard::MatchesPattern`], which also includes every non-string.
    /// Stringified predicate results use this guard for their sound
    /// raw-string mismatch subset.
    NotMatchesPattern {
        /// Values path subjected to the pattern test.
        path: ValuesPath,
        /// Literal regular expression excluded by the branch.
        pattern: String,
    },
    /// A destructured range key starts with a literal prefix. The path names
    /// the ranged collection; the predicate applies to its matching entries,
    /// not to the collection value itself.
    RangeKeyPrefix {
        /// Values path of the ranged collection.
        path: ValuesPath,
        /// Literal prefix required of the current key.
        prefix: String,
    },
    /// A destructured range key equals a literal (`if eq $key "name"`). The
    /// path names the ranged collection; the predicate selects exactly the
    /// entry with that key. Document-level lowering may only use the
    /// POSITIVE form (the key exists in the collection); the negation runs
    /// for every OTHER member and has no key-presence encoding.
    RangeKeyEquals {
        /// Values path of the ranged collection.
        path: ValuesPath,
        /// Literal key selected by the branch.
        key: String,
    },
    /// A destructured range key matches a literal regular expression
    /// (`if regexMatch "[A-Z]" $name`). The path names the ranged
    /// collection; the predicate applies per key, so lowering targets the
    /// collection's key domain (traefik's uppercase `ingressRoute` gate).
    RangeKeyMatches {
        /// Values path of the ranged collection.
        path: ValuesPath,
        /// Regular expression required of the current key.
        pattern: String,
    },
    /// Disjunction: `if or .Values.A .Values.B`
    Or {
        /// Values paths whose truthiness forms the disjunction.
        paths: Vec<ValuesPath>,
    },
    /// Disjunction whose arms may each contain a conjunction of typed guards.
    ///
    /// This preserves structural forms such as
    /// `or (and .Values.A .Values.B) (eq .Values.mode "prod")` without
    /// degrading them into truthiness checks for every mentioned path.
    AnyOf {
        /// Guard conjunctions that form the disjunction's alternatives.
        alternatives: Vec<Vec<Guard>>,
    },
    /// Body of `range .Values.X` / `range .foo` block. The referenced path is
    /// being iterated as a collection, not interpreted as a boolean-valued
    /// scalar. This should not contribute a boolean type hint downstream.
    Range {
        /// Values path used as the range source.
        path: ValuesPath,
    },
    /// Body of `with .Values.X` block. This distinguishes header binding from
    /// `if`-style truthy checks. The bound path is null-tolerant by
    /// construction because `with nil` skips the body.
    With {
        /// Values path selected as the branch context.
        path: ValuesPath,
    },
    /// Rendered via a `default ... <path>` fallback, either in prefix form
    /// (`default "x" .Values.X`) or pipeline form (`.Values.X | default "x"`).
    ///
    /// This is stronger than a plain truthy guard: the template explicitly
    /// substitutes a fallback when the path is empty/nil, so `null` is an
    /// accepted chart input for that render site even when `values.yaml` ships
    /// a non-null default.
    Default {
        /// Values path protected by a fallback.
        path: ValuesPath,
    },
    /// A `typeIs "<json type>" <path>` check in template logic.
    ///
    /// This is not a truthiness guard. It is a structural type declaration:
    /// helpers such as Bitnami's `common.tplvalues.render` explicitly branch on
    /// `typeIs "string" .value`, so callers may supply that values path as a
    /// string even when another branch renders it as a YAML object fragment.
    TypeIs {
        /// Values path subjected to the type test.
        path: ValuesPath,
        /// JSON Schema type name selected by the branch.
        schema_type: String,
    },
    /// The complement of [`Guard::TypeIs`]: the `else` arm of a type
    /// dispatch (`if typeIs "string" x … else …`).
    ///
    /// Rows need this as a first-class variant because dropping the
    /// complement collapses a type-switch partition: member reads and
    /// structural placements under the `else` would otherwise apply to
    /// EVERY type of the dispatched path.
    NotTypeIs {
        /// Values path subjected to the type test.
        path: ValuesPath,
        /// JSON Schema type name excluded by the branch.
        schema_type: String,
    },
    /// The path's RAW value is a JSON integer strictly greater than `bound`.
    ///
    /// This deliberately claims less than the Sprig coercion it stands in
    /// for: `gt (int64 .Values.x) N` also holds for numeric strings and
    /// `true`, so this guard is a SOUND SUBSET usable only where firing
    /// less often is safe (a fail-arm condition), never as an exact branch
    /// condition whose negation must also hold.
    IntGt {
        /// Values path subjected to the integer comparison.
        path: ValuesPath,
        /// Exclusive lower bound.
        bound: i64,
    },
    /// The path's RAW value is a JSON integer strictly less than `bound`.
    ///
    /// The mirror of [`Guard::IntGt`], with the same sound-subset contract:
    /// `lt (int .Values.x) N` also holds for coercible non-integers, so
    /// this guard may only strengthen positive-polarity consumers.
    IntLt {
        /// Values path subjected to the integer comparison.
        path: ValuesPath,
        /// Exclusive upper bound.
        bound: i64,
    },
    /// The collection at `path` has at most one entry.
    ///
    /// A sound SUBSET stand-in for loop-carried conditions that provably
    /// hold on a range's FIRST iteration (an empty-initialized dedup
    /// accumulator cannot shadow anything yet): with at most one member,
    /// every iteration is the first. Like [`Guard::IntGt`], it may only
    /// strengthen positive-polarity consumers.
    AtMostOneMember {
        /// Values path expected to hold the bounded collection.
        path: ValuesPath,
    },
    /// The value at `path` is a mapping with at least `bound` members —
    /// the exact meaning of `gt (keys X | len) N` (`keys` aborts on
    /// non-maps, so the render reaches the body only for maps).
    MinMembers {
        /// Values path expected to hold the mapping.
        path: ValuesPath,
        /// Inclusive minimum number of mapping members.
        bound: i64,
    },
    /// The mapping at `path` contains `key` as a literal member — Sprig
    /// `hasKey`/`dig` observability, where a present nil member IS present
    /// (cilium's removed-option guards abort on the truthy `"<nil>"`
    /// rendering of an explicit null). Contrast [`Guard::Absent`], which
    /// counts explicit null as absent for the nil-safe selector lanes.
    HasKey {
        /// Values path expected to hold a mapping.
        path: ValuesPath,
        /// Literal mapping key whose presence selects the branch.
        key: String,
    },
    /// The mapping at `path` does not contain `key`.
    ///
    /// This is the exact logical complement of [`Guard::HasKey`].
    NotHasKey {
        /// Values path expected to hold a mapping.
        path: ValuesPath,
        /// Literal mapping key whose absence selects the branch.
        key: String,
    },
    /// SOME item of the list at `path` deep-equals the scalar literal —
    /// Sprig `has LITERAL .Values.list`, the dual of the literal-list
    /// membership (`has .Values.x (list …)`). `has` returns false on a
    /// nil haystack and aborts rendering on non-lists, so the guard holds
    /// exactly for arrays carrying the literal (oauth2-proxy gates its
    /// secret keys on `has "cookie-secret" .Values.config.requiredSecretKeys`).
    ContainsEquals {
        /// Values path expected to hold a list.
        path: ValuesPath,
        /// Literal that at least one list item must equal.
        value: GuardValue,
    },
    /// Some iterated item of the collection at `path` has `member` equal to
    /// `value`. This is the quantified result of a monotone Boolean local
    /// set inside a range under an equality test.
    ContainsMemberEquals {
        /// Values path expected to hold the iterated collection.
        path: ValuesPath,
        /// Member name compared within each collection item.
        member: String,
        /// Literal that at least one member must equal.
        value: GuardValue,
    },
    /// Some iterated item of the collection at `path` has a Helm-truthy
    /// `member`. This is the quantified result of a monotone Boolean local
    /// set inside a range under a truthiness test.
    ContainsTruthyMember {
        /// Values path expected to hold the iterated collection.
        path: ValuesPath,
        /// Member whose truthiness selects the sentinel state.
        member: String,
    },
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
            | Self::NotMatchesPattern { .. }
            | Self::RangeKeyPrefix { .. }
            | Self::RangeKeyEquals { .. }
            | Self::RangeKeyMatches { .. }
            | Self::Range { .. }
            | Self::With { .. }
            | Self::Default { .. }
            | Self::TypeIs { .. }
            | Self::NotTypeIs { .. }
            | Self::IntGt { .. }
            | Self::IntLt { .. }
            | Self::AtMostOneMember { .. }
            | Self::MinMembers { .. }
            | Self::HasKey { .. }
            | Self::NotHasKey { .. }
            | Self::ContainsEquals { .. }
            | Self::ContainsMemberEquals { .. }
            | Self::ContainsTruthyMember { .. } => {}
        }
    }

    /// Return all `.Values.*` paths referenced by this guard.
    #[must_use]
    pub fn value_paths(&self) -> Vec<ValuesPath> {
        match self {
            Guard::Truthy { path }
            | Guard::Not { path }
            | Guard::Eq { path, .. }
            | Guard::NotEq { path, .. }
            | Guard::Absent { path }
            | Guard::MatchesPattern { path, .. }
            | Guard::NotMatchesPattern { path, .. }
            | Guard::RangeKeyPrefix { path, .. }
            | Guard::RangeKeyEquals { path, .. }
            | Guard::RangeKeyMatches { path, .. }
            | Guard::Range { path }
            | Guard::With { path }
            | Guard::Default { path }
            | Guard::TypeIs { path, .. }
            | Guard::NotTypeIs { path, .. }
            | Guard::IntGt { path, .. }
            | Guard::IntLt { path, .. }
            | Guard::AtMostOneMember { path }
            | Guard::MinMembers { path, .. }
            | Guard::HasKey { path, .. }
            | Guard::NotHasKey { path, .. }
            | Guard::ContainsEquals { path, .. }
            | Guard::ContainsMemberEquals { path, .. }
            | Guard::ContainsTruthyMember { path, .. } => {
                vec![path.clone()]
            }
            Guard::Or { paths } => paths.clone(),
            Guard::AnyOf { alternatives } => alternatives
                .iter()
                .flat_map(|alternative| alternative.iter().flat_map(Guard::value_paths))
                .collect(),
        }
    }

    /// Rewrite value paths carried by this guard.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "keeping the exhaustive path rewrite in one match makes variant coverage auditable"
    )]
    pub fn map_value_paths<F>(self, map: &mut F) -> Self
    where
        F: FnMut(ValuesPath) -> ValuesPath,
    {
        match self {
            Guard::Truthy { path } => Guard::Truthy {
                path: map_values_path(&path, map),
            },
            Guard::Not { path } => Guard::Not {
                path: map_values_path(&path, map),
            },
            Guard::Eq { path, value } => Guard::Eq {
                path: map_values_path(&path, map),
                value,
            },
            Guard::NotEq { path, value } => Guard::NotEq {
                path: map_values_path(&path, map),
                value,
            },
            Guard::Absent { path } => Guard::Absent {
                path: map_values_path(&path, map),
            },
            Guard::MatchesPattern {
                path,
                pattern,
                templated,
            } => Guard::MatchesPattern {
                path: map_values_path(&path, map),
                pattern,
                templated,
            },
            Guard::NotMatchesPattern { path, pattern } => Guard::NotMatchesPattern {
                path: map_values_path(&path, map),
                pattern,
            },
            Guard::RangeKeyEquals { path, key } => Guard::RangeKeyEquals {
                path: map_values_path(&path, map),
                key,
            },
            Guard::RangeKeyPrefix { path, prefix } => Guard::RangeKeyPrefix {
                path: map_values_path(&path, map),
                prefix,
            },
            Guard::RangeKeyMatches { path, pattern } => Guard::RangeKeyMatches {
                path: map_values_path(&path, map),
                pattern,
            },
            Guard::Or { paths } => Guard::Or {
                paths: paths
                    .into_iter()
                    .map(|path| map_values_path(&path, map))
                    .collect(),
            },
            Guard::AnyOf { alternatives } => Guard::AnyOf {
                alternatives: map_guard_alternatives(alternatives, map),
            },
            Guard::Range { path } => Guard::Range {
                path: map_values_path(&path, map),
            },
            Guard::With { path } => Guard::With {
                path: map_values_path(&path, map),
            },
            Guard::Default { path } => Guard::Default {
                path: map_values_path(&path, map),
            },
            Guard::TypeIs { path, schema_type } => Guard::TypeIs {
                path: map_values_path(&path, map),
                schema_type,
            },
            Guard::NotTypeIs { path, schema_type } => Guard::NotTypeIs {
                path: map_values_path(&path, map),
                schema_type,
            },
            Guard::IntGt { path, bound } => Guard::IntGt {
                path: map_values_path(&path, map),
                bound,
            },
            Guard::IntLt { path, bound } => Guard::IntLt {
                path: map_values_path(&path, map),
                bound,
            },
            Guard::AtMostOneMember { path } => Guard::AtMostOneMember {
                path: map_values_path(&path, map),
            },
            Guard::MinMembers { path, bound } => Guard::MinMembers {
                path: map_values_path(&path, map),
                bound,
            },
            Guard::HasKey { path, key } => Guard::HasKey {
                path: map_values_path(&path, map),
                key,
            },
            Guard::NotHasKey { path, key } => Guard::NotHasKey {
                path: map_values_path(&path, map),
                key,
            },
            Guard::ContainsEquals { path, value } => Guard::ContainsEquals {
                path: map_values_path(&path, map),
                value,
            },
            Guard::ContainsMemberEquals {
                path,
                member,
                value,
            } => Guard::ContainsMemberEquals {
                path: map_values_path(&path, map),
                member,
                value,
            },
            Guard::ContainsTruthyMember { path, member } => Guard::ContainsTruthyMember {
                path: map_values_path(&path, map),
                member,
            },
        }
    }
}

fn map_values_path<F>(path: &ValuesPath, map: &mut F) -> ValuesPath
where
    F: FnMut(ValuesPath) -> ValuesPath,
{
    map(path.clone())
}

fn map_guard_alternatives<F>(alternatives: Vec<Vec<Guard>>, map: &mut F) -> Vec<Vec<Guard>>
where
    F: FnMut(ValuesPath) -> ValuesPath,
{
    alternatives
        .into_iter()
        .map(|alternative| {
            alternative
                .into_iter()
                .map(|guard| guard.map_value_paths(map))
                .collect()
        })
        .collect()
}
