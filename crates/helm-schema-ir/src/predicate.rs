use crate::{Guard, GuardValue};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Predicate {
    True,
    False,
    Atom(PredicateAtom),
    Not(Box<Predicate>),
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PredicateAtom {
    Truthy { path: String },
    Eq { path: String, value: GuardValue },
    NotEq { path: String, value: GuardValue },
    Absent { path: String },
    Range { path: String },
    With { path: String },
    Default { path: String },
    TypeIs { path: String, schema_type: String },
}

impl From<Guard> for Predicate {
    fn from(guard: Guard) -> Self {
        match guard {
            Guard::Truthy { path } => Self::Atom(PredicateAtom::Truthy { path }),
            Guard::Not { path } => Self::Not(Box::new(Self::Atom(PredicateAtom::Truthy { path }))),
            Guard::Eq { path, value } => Self::Atom(PredicateAtom::Eq { path, value }),
            Guard::NotEq { path, value } => Self::Atom(PredicateAtom::NotEq { path, value }),
            Guard::Absent { path } => Self::Atom(PredicateAtom::Absent { path }),
            Guard::Or { paths } => Self::Or(
                paths
                    .into_iter()
                    .map(|path| Self::Atom(PredicateAtom::Truthy { path }))
                    .collect(),
            ),
            Guard::AnyOf { alternatives } => Self::Or(
                alternatives
                    .into_iter()
                    .map(|alternative| Self::all(alternative.into_iter().map(Self::from).collect()))
                    .collect(),
            ),
            Guard::Range { path } => Self::Atom(PredicateAtom::Range { path }),
            Guard::With { path } => Self::Atom(PredicateAtom::With { path }),
            Guard::Default { path } => Self::Atom(PredicateAtom::Default { path }),
            Guard::TypeIs { path, schema_type } => {
                Self::Atom(PredicateAtom::TypeIs { path, schema_type })
            }
        }
    }
}

impl Predicate {
    pub(crate) fn truthy_path(path: impl Into<String>) -> Self {
        Self::Atom(PredicateAtom::Truthy { path: path.into() })
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
            Self::Atom(atom) => atom.contract_guards(),
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
        Predicate::Atom(PredicateAtom::Truthy { path }) => vec![Guard::Not { path: path.clone() }],
        Predicate::Atom(PredicateAtom::Eq { path, value }) => vec![Guard::NotEq {
            path: path.clone(),
            value: value.clone(),
        }],
        Predicate::Atom(PredicateAtom::NotEq { path, value }) => vec![Guard::Eq {
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

impl PredicateAtom {
    fn contract_guards(&self) -> Vec<Guard> {
        let guard = match self {
            Self::Truthy { path } => Guard::Truthy { path: path.clone() },
            Self::Eq { path, value } => Guard::Eq {
                path: path.clone(),
                value: value.clone(),
            },
            Self::NotEq { path, value } => Guard::NotEq {
                path: path.clone(),
                value: value.clone(),
            },
            Self::Absent { path } => Guard::Absent { path: path.clone() },
            Self::Range { path } => Guard::Range { path: path.clone() },
            Self::With { path } => Guard::With { path: path.clone() },
            Self::Default { path } => Guard::Default { path: path.clone() },
            Self::TypeIs { path, schema_type } => Guard::TypeIs {
                path: path.clone(),
                schema_type: schema_type.clone(),
            },
        };
        vec![guard]
    }
}

#[cfg(test)]
mod tests {
    use super::Predicate;
    use crate::{Guard, GuardValue};
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn or_truthy_predicate_projects_to_or_guard() {
        let predicate = Predicate::from(Guard::Or {
            paths: vec!["first".to_string(), "second".to_string()],
        });

        sim_assert_eq!(
            have: predicate.contract_guards(),
            want: vec![Guard::Or {
                paths: vec!["first".to_string(), "second".to_string()]
            }]
        );
    }

    #[test]
    fn negated_truthy_predicate_projects_to_not_guard() {
        let predicate = Predicate::from(Guard::Truthy {
            path: "enabled".to_string(),
        })
        .negated();

        sim_assert_eq!(
            have: predicate.contract_guards(),
            want: vec![Guard::Not {
                path: "enabled".to_string()
            }]
        );
    }

    #[test]
    fn double_negated_truthy_predicate_projects_to_truthy_guard() {
        let predicate = Predicate::from(Guard::Truthy {
            path: "enabled".to_string(),
        })
        .negated()
        .negated();

        sim_assert_eq!(
            have: predicate.contract_guards(),
            want: vec![Guard::Truthy {
                path: "enabled".to_string()
            }]
        );
    }

    #[test]
    fn negated_eq_predicate_projects_to_not_eq_guard() {
        let predicate = Predicate::Not(Box::new(Predicate::from(Guard::Eq {
            path: "mode".to_string(),
            value: GuardValue::string("prod"),
        })));

        sim_assert_eq!(
            have: predicate.contract_guards(),
            want: vec![Guard::NotEq {
                path: "mode".to_string(),
                value: GuardValue::string("prod"),
            }]
        );
    }

    #[test]
    fn not_eq_predicate_projects_to_not_eq_guard() {
        let predicate = Predicate::from(Guard::NotEq {
            path: "mode".to_string(),
            value: GuardValue::string("disabled"),
        });

        sim_assert_eq!(
            have: predicate.contract_guards(),
            want: vec![Guard::NotEq {
                path: "mode".to_string(),
                value: GuardValue::string("disabled"),
            }]
        );
    }

    #[test]
    fn mixed_or_predicate_projects_to_structural_any_of_guard() {
        let predicate = Predicate::Or(vec![
            Predicate::from(Guard::Truthy {
                path: "first".to_string(),
            }),
            Predicate::from(Guard::Eq {
                path: "mode".to_string(),
                value: GuardValue::string("prod"),
            }),
        ]);

        sim_assert_eq!(
            have: predicate.contract_guards(),
            want: vec![Guard::AnyOf {
                alternatives: vec![
                    vec![Guard::Truthy {
                        path: "first".to_string(),
                    }],
                    vec![Guard::Eq {
                        path: "mode".to_string(),
                        value: GuardValue::string("prod"),
                    }],
                ],
            }]
        );
    }

    #[test]
    fn contract_guard_stack_dedupes_projected_guards() {
        let predicate = Predicate::from(Guard::Truthy {
            path: "enabled".to_string(),
        });

        sim_assert_eq!(
            have: Predicate::contract_guard_stack(&[predicate.clone(), predicate]),
            want: vec![Guard::Truthy {
                path: "enabled".to_string()
            }]
        );
    }
}
