use crate::Guard;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Predicate {
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
            guard => Self::Guard(guard),
        }
    }
}

impl Predicate {
    pub(crate) fn truthy_path(path: impl Into<String>) -> Self {
        Self::Guard(Guard::Truthy { path: path.into() })
    }

    pub(crate) fn all(predicates: Vec<Self>) -> Self {
        match predicates.as_slice() {
            [] => Self::True,
            [predicate] => predicate.clone(),
            _ => Self::And(predicates),
        }
    }

    pub(crate) fn negated(&self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Not(inner) => inner.as_ref().clone(),
            other => Self::Not(Box::new(other.clone())),
        }
    }

    pub(crate) fn is_trivial(&self) -> bool {
        matches!(self, Self::True | Self::False)
    }

    pub(crate) fn contract_guards(&self) -> Vec<Guard> {
        match self {
            Self::True | Self::False => Vec::new(),
            Self::Guard(guard) => vec![guard.clone()],
            Self::Not(inner) => negated_contract_guards(inner),
            Self::And(predicates) => predicates.iter().flat_map(Self::contract_guards).collect(),
            Self::Or(predicates) => or_contract_guards(predicates),
        }
    }

    pub(crate) fn contract_guard_stack(predicates: &[Self]) -> Vec<Guard> {
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
        Predicate::Not(inner) => inner.contract_guards(),
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
