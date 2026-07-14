use std::collections::BTreeSet;

use crate::Guard;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Predicate {
    True,
    False,
    Guard(Guard),
    Not(Box<Predicate>),
    And(Vec<Predicate>),
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
    pub fn truthy_path(path: impl Into<String>) -> Self {
        Self::Guard(Guard::Truthy { path: path.into() })
    }

    pub fn all(predicates: Vec<Self>) -> Self {
        match predicates.as_slice() {
            [] => Self::True,
            [predicate] => predicate.clone(),
            _ => Self::And(predicates),
        }
    }

    pub fn negated(&self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Not(inner) => inner.as_ref().clone(),
            other => Self::Not(Box::new(other.clone())),
        }
    }

    pub fn is_trivial(&self) -> bool {
        matches!(self, Self::True | Self::False)
    }

    pub fn value_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        self.collect_value_paths(&mut paths);
        paths
    }

    pub fn with_context_predicates(self) -> Vec<Self> {
        match self {
            Self::True => Vec::new(),
            Self::False => vec![Self::False],
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
            Self::Guard(Guard::NotEq { path, value }) => vec![
                Self::from(Guard::With { path: path.clone() }),
                Self::from(Guard::NotEq { path, value }),
            ],
            Self::Guard(
                Guard::Range { .. }
                | Guard::Absent { .. }
                | Guard::With { .. }
                | Guard::Default { .. }
                | Guard::TypeIs { .. }
                | Guard::NotTypeIs { .. }
                | Guard::Not { .. }
                | Guard::Or { .. }
                | Guard::AnyOf { .. },
            ) => vec![self],
        }
    }

    pub fn conditionally_optional_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        self.collect_conditionally_optional_paths(&mut paths);
        paths
    }

    pub fn contract_guards(&self) -> Vec<Guard> {
        match self {
            Self::True | Self::False => Vec::new(),
            Self::Guard(guard) => vec![guard.clone()],
            Self::Not(inner) => negated_contract_guards(inner),
            Self::And(predicates) => predicates.iter().flat_map(Self::contract_guards).collect(),
            Self::Or(predicates) => or_contract_guards(predicates),
        }
    }

    /// Whether [`Self::contract_guards`] represents this predicate
    /// EXACTLY: the flattened guard conjunction selects the same states.
    /// Complex negations (`¬(a ∨ (b ∧ c))`) flatten to NOTHING, which an
    /// `And` flatten silently drops — a fail conjunction missing such a
    /// conjunct negates into states the validator never rejects, so
    /// callers keep inexact conjuncts as raw predicates instead.
    #[must_use]
    pub fn contract_guards_are_exact(&self) -> bool {
        match self {
            Self::True | Self::Guard(_) => true,
            Self::False => false,
            Self::Not(inner) => match inner.as_ref() {
                Self::Guard(
                    Guard::Truthy { .. }
                    | Guard::Eq { .. }
                    | Guard::NotEq { .. }
                    | Guard::TypeIs { .. }
                    | Guard::NotTypeIs { .. },
                ) => true,
                Self::Not(inner) => inner.contract_guards_are_exact(),
                _ => false,
            },
            Self::And(predicates) | Self::Or(predicates) => {
                predicates.iter().all(Self::contract_guards_are_exact)
            }
        }
    }

    fn collect_value_paths(&self, out: &mut BTreeSet<String>) {
        match self {
            Self::True | Self::False => {}
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
            | Self::Guard(
                Guard::Truthy { .. }
                | Guard::Eq { .. }
                | Guard::Range { .. }
                | Guard::With { .. }
                | Guard::Default { .. }
                | Guard::TypeIs { .. }
                | Guard::NotTypeIs { .. }
                | Guard::Not { .. }
                | Guard::Or { .. }
                | Guard::AnyOf { .. },
            ) => {}
        }
    }

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

    #[must_use]
    pub fn map_value_paths<F>(self, map: &mut F) -> Self
    where
        F: FnMut(&str) -> String,
    {
        match self {
            Self::True => Self::True,
            Self::False => Self::False,
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

fn negated_contract_guards(inner: &Predicate) -> Vec<Guard> {
    match inner {
        Predicate::Guard(Guard::Truthy { path }) => vec![Guard::Not { path: path.clone() }],
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
        _ => Vec::new(),
    }
}

fn or_contract_guards(predicates: &[Predicate]) -> Vec<Guard> {
    let mut alternatives = predicates
        .iter()
        .map(Predicate::contract_guards)
        .collect::<Vec<_>>();

    if alternatives.iter().any(Vec::is_empty) {
        return Vec::new();
    }
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
