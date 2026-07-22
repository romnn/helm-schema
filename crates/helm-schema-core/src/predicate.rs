use std::collections::BTreeSet;

use crate::Guard;

/// Typed Boolean formula recovered from template control flow.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Predicate {
    /// Formula that holds for every input.
    True,
    /// Formula that holds for no input.
    False,
    /// A control condition whose exact relation could not be lowered.
    ///
    /// The paths remain available for diagnostics and conservative attribution, but consumers
    /// must not turn this marker into a narrowing schema condition.
    Approximate {
        /// Stable description of the expression shape that could not be lowered.
        marker: String,
        /// Values paths mentioned by the unlowerable expression.
        paths: BTreeSet<String>,
        /// Guards whose conjunction IMPLIES the real condition (a sound
        /// subset). Usable only in POSITIVE polarity where firing less
        /// often is safe — a fail-arm's outer condition — never through a
        /// negation, which would invert the containment. Empty when no
        /// bounded strengthening was recognized.
        sound_subset: Vec<Guard>,
    },
    /// Exactly lowerable atomic guard.
    Guard(Guard),
    /// Logical negation of a predicate.
    Not(Box<Predicate>),
    /// Conjunction of every enclosed predicate.
    And(Vec<Predicate>),
    /// Disjunction of the enclosed predicates.
    Or(Vec<Predicate>),
}

impl From<Guard> for Predicate {
    fn from(guard: Guard) -> Self {
        match guard {
            Guard::Not { path } => Self::Not(Box::new(Self::truthy_path(path))),
            Guard::Or { paths } => Self::Or(paths.into_iter().map(Self::truthy_path).collect()),
            Guard::AnyOf { alternatives } => Self::Or(
                alternatives
                    .into_iter()
                    .map(|alternative| Self::all(alternative.into_iter().map(Self::from).collect()))
                    .collect(),
            ),
            Guard::NotTypeIs { path, schema_type } => {
                Self::Not(Box::new(Self::Guard(Guard::TypeIs { path, schema_type })))
            }
            guard => Self::Guard(guard),
        }
    }
}

impl Predicate {
    /// Creates an atomic truthiness predicate for a values path.
    pub fn truthy_path(path: impl Into<String>) -> Self {
        Self::Guard(Guard::Truthy { path: path.into() })
    }

    /// Marks an unlowerable condition without inventing a relation between its paths.
    pub fn approximate(marker: impl Into<String>, paths: BTreeSet<String>) -> Self {
        Self::Approximate {
            marker: marker.into(),
            paths,
            sound_subset: Vec::new(),
        }
    }

    /// Marks an unlowerable condition that still admits a bounded sound
    /// strengthening: `guards` hold only in states where the real condition
    /// holds too.
    pub fn approximate_with_sound_subset(
        marker: impl Into<String>,
        paths: BTreeSet<String>,
        sound_subset: Vec<Guard>,
    ) -> Self {
        Self::Approximate {
            marker: marker.into(),
            paths,
            sound_subset,
        }
    }

    /// Normalizes a conjunction, collapsing empty and singleton formulas.
    #[must_use]
    pub fn all(predicates: Vec<Self>) -> Self {
        match predicates.as_slice() {
            [] => Self::True,
            [predicate] => predicate.clone(),
            _ => Self::And(predicates),
        }
    }

    /// Returns the logical complement without retaining redundant double negation.
    #[must_use]
    pub fn negated(&self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Not(inner) => inner.as_ref().clone(),
            other => Self::Not(Box::new(other.clone())),
        }
    }

    /// Reports whether the predicate is the constant `true` or `false` formula.
    #[must_use]
    pub fn is_trivial(&self) -> bool {
        matches!(self, Self::True | Self::False)
    }

    /// Whether this predicate contains a condition that could not be lowered exactly.
    /// Returns every values path referenced by the formula.
    #[must_use]
    pub fn contains_approximation(&self) -> bool {
        match self {
            Self::Approximate { .. } => true,
            Self::Not(inner) => inner.contains_approximation(),
            Self::And(predicates) | Self::Or(predicates) => {
                predicates.iter().any(Self::contains_approximation)
            }
            Self::True | Self::False | Self::Guard(_) => false,
        }
    }

    /// Returns every values path referenced by the formula.
    #[must_use]
    pub fn value_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        self.collect_value_paths(&mut paths);
        paths
    }

    /// Expands header predicates into the context-selection facts active in their bodies.
    pub fn with_context_predicates(self) -> Vec<Self> {
        match self {
            Self::True => Vec::new(),
            Self::False => vec![Self::False],
            Self::Approximate { .. }
            | Self::Guard(
                Guard::Range { .. }
                | Guard::RangeKeyPrefix { .. }
                | Guard::RangeKeyEquals { .. }
                | Guard::RangeKeyMatches { .. }
                | Guard::Absent { .. }
                | Guard::With { .. }
                | Guard::Default { .. }
                | Guard::TypeIs { .. }
                | Guard::NotTypeIs { .. }
                | Guard::Not { .. }
                | Guard::Or { .. }
                | Guard::AnyOf { .. }
                | Guard::IntGt { .. }
                | Guard::IntLt { .. }
                | Guard::AtMostOneMember { .. }
                | Guard::MinMembers { .. }
                | Guard::HasKey { .. }
                | Guard::ContainsEquals { .. },
            ) => vec![self],
            Self::And(predicates) => predicates
                .into_iter()
                .flat_map(Self::with_context_predicates)
                .collect(),
            Self::Guard(Guard::Truthy { path }) => vec![Self::from(Guard::With { path })],
            Self::Or(predicates) => {
                let paths: Option<Vec<String>> = predicates
                    .iter()
                    .map(|predicate| match predicate {
                        Self::Guard(Guard::Truthy { path }) => Some(path.clone()),
                        _ => None,
                    })
                    .collect();
                let Some(paths) = paths else {
                    return vec![Self::Or(predicates)];
                };
                let mut out: Vec<Self> = paths
                    .iter()
                    .map(|path| Self::from(Guard::With { path: path.clone() }))
                    .collect();
                out.push(Self::Or(paths.into_iter().map(Self::truthy_path).collect()));
                out
            }
            Self::Not(inner) => match inner.as_ref() {
                Self::Guard(Guard::Truthy { path }) => vec![
                    Self::from(Guard::With { path: path.clone() }),
                    Self::Not(inner),
                ],
                _ => vec![Self::Not(inner)],
            },
            Self::Guard(Guard::Eq { path, value }) => vec![
                Self::from(Guard::With { path: path.clone() }),
                Self::from(Guard::Eq { path, value }),
            ],
            Self::Guard(Guard::MatchesPattern {
                path,
                pattern,
                templated,
            }) => vec![
                Self::from(Guard::With { path: path.clone() }),
                Self::from(Guard::MatchesPattern {
                    path,
                    pattern,
                    templated,
                }),
            ],
            Self::Guard(Guard::NotEq { path, value }) => vec![
                Self::from(Guard::With { path: path.clone() }),
                Self::from(Guard::NotEq { path, value }),
            ],
        }
    }

    /// Returns values paths whose branch structure permits them to be absent.
    #[must_use]
    pub fn conditionally_optional_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        self.collect_conditionally_optional_paths(&mut paths);
        paths
    }

    /// Projects this formula into the contract guard vocabulary.
    pub fn contract_guards(&self) -> Vec<Guard> {
        match self {
            Self::True | Self::False | Self::Approximate { .. } => Vec::new(),
            Self::Guard(guard) => vec![guard.clone()],
            Self::Not(inner) => negated_contract_guards(inner),
            Self::And(predicates) => predicates.iter().flat_map(Self::contract_guards).collect(),
            Self::Or(predicates) => or_contract_guards(predicates),
        }
    }

    /// Whether [`Self::contract_guards`] represents this predicate
    /// EXACTLY: the flattened guard conjunction selects the same states.
    /// Negations distribute by De Morgan down to negatable guard leaves;
    /// a negation reaching a leaf the vocabulary cannot flip flattens to
    /// NOTHING, which an `And` flatten would silently drop — a fail
    /// conjunction missing such a conjunct negates into states the
    /// validator never rejects, so callers keep those conjuncts as raw
    /// predicates instead.
    #[must_use]
    pub fn contract_guards_are_exact(&self) -> bool {
        match self {
            Self::True | Self::Guard(_) => true,
            Self::False | Self::Approximate { .. } => false,
            Self::Not(inner) => negation_flattens_exactly(inner),
            Self::And(predicates) | Self::Or(predicates) => {
                predicates.iter().all(Self::contract_guards_are_exact)
            }
        }
    }

    fn collect_value_paths(&self, out: &mut BTreeSet<String>) {
        match self {
            Self::True | Self::False => {}
            Self::Approximate { paths, .. } => out.extend(paths.iter().cloned()),
            Self::Guard(guard) => {
                for path in guard.value_paths() {
                    out.insert(path.to_string());
                }
            }
            Self::Not(inner) => inner.collect_value_paths(out),
            Self::And(predicates) | Self::Or(predicates) => {
                for predicate in predicates {
                    predicate.collect_value_paths(out);
                }
            }
        }
    }

    fn collect_conditionally_optional_paths(&self, out: &mut BTreeSet<String>) {
        match self {
            Self::Guard(Guard::NotEq { path, .. } | Guard::Absent { path }) => {
                out.insert(path.clone());
            }
            Self::Not(inner) => match inner.as_ref() {
                Self::Guard(Guard::Truthy { path }) => {
                    out.insert(path.clone());
                }
                _ => inner.collect_conditionally_optional_paths(out),
            },
            Self::Or(predicates) => {
                for predicate in predicates {
                    out.extend(predicate.value_paths());
                }
            }
            Self::And(predicates) => {
                for predicate in predicates {
                    predicate.collect_conditionally_optional_paths(out);
                }
            }
            Self::True
            | Self::False
            | Self::Approximate { .. }
            | Self::Guard(
                Guard::Truthy { .. }
                | Guard::Eq { .. }
                | Guard::MatchesPattern { .. }
                | Guard::RangeKeyPrefix { .. }
                | Guard::RangeKeyEquals { .. }
                | Guard::RangeKeyMatches { .. }
                | Guard::Range { .. }
                | Guard::With { .. }
                | Guard::Default { .. }
                | Guard::TypeIs { .. }
                | Guard::NotTypeIs { .. }
                | Guard::Not { .. }
                | Guard::Or { .. }
                | Guard::AnyOf { .. }
                | Guard::IntGt { .. }
                | Guard::IntLt { .. }
                | Guard::AtMostOneMember { .. }
                | Guard::MinMembers { .. }
                | Guard::HasKey { .. }
                | Guard::ContainsEquals { .. },
            ) => {}
        }
    }

    /// Projects a predicate stack into a deduplicated guard conjunction.
    #[must_use]
    pub fn contract_guard_stack(predicates: &[Self]) -> Vec<Guard> {
        let mut guards = Vec::new();
        for predicate in predicates {
            for guard in predicate.contract_guards() {
                if !guards.contains(&guard) {
                    guards.push(guard);
                }
            }
        }
        guards
    }

    /// Rewrites every values path carried by this formula.
    #[must_use]
    pub fn map_value_paths<F>(self, map: &mut F) -> Self
    where
        F: FnMut(&str) -> String,
    {
        match self {
            Self::True => Self::True,
            Self::False => Self::False,
            Self::Approximate {
                marker,
                paths,
                sound_subset,
            } => Self::Approximate {
                marker,
                paths: paths.into_iter().map(|path| map(&path)).collect(),
                sound_subset: sound_subset
                    .into_iter()
                    .map(|guard| guard.map_value_paths(map))
                    .collect(),
            },
            Self::Guard(guard) => Self::Guard(guard.map_value_paths(map)),
            Self::Not(inner) => Self::Not(Box::new(inner.map_value_paths(map))),
            Self::And(predicates) => Self::And(
                predicates
                    .into_iter()
                    .map(|predicate| predicate.map_value_paths(map))
                    .collect(),
            ),
            Self::Or(predicates) => Self::Or(
                predicates
                    .into_iter()
                    .map(|predicate| predicate.map_value_paths(map))
                    .collect(),
            ),
        }
    }
}

/// Whether [`negated_contract_guards`] flattens `¬inner` exactly: every
/// De Morgan leaf must be a negatable guard (`True`/`False` leaves are
/// excluded — the guard vocabulary cannot spell a constant).
fn negation_flattens_exactly(inner: &Predicate) -> bool {
    match inner {
        Predicate::Guard(
            Guard::Truthy { .. }
            | Guard::With { .. }
            | Guard::Not { .. }
            | Guard::Or { .. }
            | Guard::Eq { .. }
            | Guard::NotEq { .. }
            | Guard::TypeIs { .. }
            | Guard::NotTypeIs { .. },
        ) => true,
        Predicate::Not(inner) => inner.contract_guards_are_exact(),
        Predicate::And(predicates) | Predicate::Or(predicates) => {
            predicates.iter().all(negation_flattens_exactly)
        }
        _ => false,
    }
}

fn negated_contract_guards(inner: &Predicate) -> Vec<Guard> {
    match inner {
        Predicate::Guard(Guard::Truthy { path } | Guard::With { path }) => {
            vec![Guard::Not { path: path.clone() }]
        }
        Predicate::Guard(Guard::Not { path }) => vec![Guard::Truthy { path: path.clone() }],
        Predicate::Guard(Guard::Or { paths }) => paths
            .iter()
            .map(|path| Guard::Not { path: path.clone() })
            .collect(),
        Predicate::Guard(Guard::Eq { path, value }) => vec![Guard::NotEq {
            path: path.clone(),
            value: value.clone(),
        }],
        Predicate::Guard(Guard::NotEq { path, value }) => vec![Guard::Eq {
            path: path.clone(),
            value: value.clone(),
        }],
        Predicate::Guard(Guard::TypeIs { path, schema_type }) => vec![Guard::NotTypeIs {
            path: path.clone(),
            schema_type: schema_type.clone(),
        }],
        Predicate::Guard(Guard::NotTypeIs { path, schema_type }) => vec![Guard::TypeIs {
            path: path.clone(),
            schema_type: schema_type.clone(),
        }],
        Predicate::Not(inner) => inner.contract_guards(),
        // ¬(p₁ ∨ … ∨ pₙ) = ¬p₁ ∧ … ∧ ¬pₙ: a plain conjunction, exact only
        // when every disjunct negates exactly (an empty flatten anywhere
        // abstains the whole negation instead of silently dropping it).
        Predicate::Or(predicates) => {
            let mut guards = Vec::new();
            for predicate in predicates {
                let negated = negated_contract_guards(predicate);
                if negated.is_empty() {
                    return Vec::new();
                }
                guards.extend(negated);
            }
            guards
        }
        // ¬(p₁ ∧ … ∧ pₙ) = ¬p₁ ∨ … ∨ ¬pₙ: one alternative per conjunct,
        // sharing the disjunction normalization of the positive `Or` lane.
        Predicate::And(predicates) => {
            let mut alternatives = Vec::new();
            for predicate in predicates {
                let negated = negated_contract_guards(predicate);
                if negated.is_empty() {
                    return Vec::new();
                }
                alternatives.push(negated);
            }
            alternatives_to_guards(alternatives)
        }
        _ => Vec::new(),
    }
}

fn or_contract_guards(predicates: &[Predicate]) -> Vec<Guard> {
    let alternatives = predicates
        .iter()
        .map(Predicate::contract_guards)
        .collect::<Vec<_>>();

    if alternatives.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    alternatives_to_guards(alternatives)
}

/// Normalize a disjunction of guard conjunctions into guard form: a single
/// alternative collapses to its conjunction, all-truthy alternatives become
/// the flat [`Guard::Or`], anything else the general [`Guard::AnyOf`].
fn alternatives_to_guards(mut alternatives: Vec<Vec<Guard>>) -> Vec<Guard> {
    for alternative in &mut alternatives {
        alternative.sort();
        alternative.dedup();
    }
    alternatives.sort();
    alternatives.dedup();

    if alternatives.len() == 1 {
        return alternatives.pop().unwrap_or_default();
    }

    if let Some(paths) = truthy_or_paths(&alternatives) {
        return vec![Guard::Or { paths }];
    }

    vec![Guard::AnyOf { alternatives }]
}

fn truthy_or_paths(alternatives: &[Vec<Guard>]) -> Option<Vec<String>> {
    alternatives
        .iter()
        .map(|alternative| match alternative.as_slice() {
            [Guard::Truthy { path }] => Some(path.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/predicate.rs"]
mod tests;
