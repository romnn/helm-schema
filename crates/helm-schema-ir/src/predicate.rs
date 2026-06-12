use crate::Guard;

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
    Eq { path: String, value: String },
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
            Guard::Or { paths } => Self::Or(
                paths
                    .into_iter()
                    .map(|path| Self::Atom(PredicateAtom::Truthy { path }))
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

    pub(crate) fn compatibility_guards(&self) -> Vec<Guard> {
        match self {
            Self::True | Self::False => Vec::new(),
            Self::Atom(atom) => atom.compatibility_guards(),
            Self::Not(inner) => match inner.as_ref() {
                Self::Atom(PredicateAtom::Truthy { path }) => {
                    vec![Guard::Not { path: path.clone() }]
                }
                _ => Vec::new(),
            },
            Self::And(predicates) => predicates
                .iter()
                .flat_map(Self::compatibility_guards)
                .collect(),
            Self::Or(predicates) => {
                let paths: Option<Vec<String>> = predicates
                    .iter()
                    .map(|predicate| match predicate {
                        Self::Atom(PredicateAtom::Truthy { path }) => Some(path.clone()),
                        _ => None,
                    })
                    .collect();
                paths
                    .map(|paths| vec![Guard::Or { paths }])
                    .unwrap_or_default()
            }
        }
    }

    pub(crate) fn compatibility_guard_stack(predicates: &[Self]) -> Vec<Guard> {
        let mut guards = Vec::new();
        for predicate in predicates {
            for guard in predicate.compatibility_guards() {
                if !guards.contains(&guard) {
                    guards.push(guard);
                }
            }
        }
        guards
    }
}

impl PredicateAtom {
    fn compatibility_guards(&self) -> Vec<Guard> {
        let guard = match self {
            Self::Truthy { path } => Guard::Truthy { path: path.clone() },
            Self::Eq { path, value } => Guard::Eq {
                path: path.clone(),
                value: value.clone(),
            },
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
    use crate::Guard;

    #[test]
    fn or_truthy_predicate_projects_to_or_guard() {
        let predicate = Predicate::from(Guard::Or {
            paths: vec!["first".to_string(), "second".to_string()],
        });

        assert_eq!(
            predicate.compatibility_guards(),
            vec![Guard::Or {
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

        assert_eq!(
            predicate.compatibility_guards(),
            vec![Guard::Not {
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

        assert_eq!(
            predicate.compatibility_guards(),
            vec![Guard::Truthy {
                path: "enabled".to_string()
            }]
        );
    }

    #[test]
    fn unsupported_negated_predicate_abstains_from_flat_guard_projection() {
        let predicate = Predicate::Not(Box::new(Predicate::from(Guard::Eq {
            path: "mode".to_string(),
            value: "prod".to_string(),
        })));

        assert_eq!(predicate.compatibility_guards(), Vec::new());
    }

    #[test]
    fn unsupported_or_predicate_abstains_from_flat_guard_projection() {
        let predicate = Predicate::Or(vec![
            Predicate::from(Guard::Truthy {
                path: "first".to_string(),
            }),
            Predicate::from(Guard::Eq {
                path: "mode".to_string(),
                value: "prod".to_string(),
            }),
        ]);

        assert_eq!(predicate.compatibility_guards(), Vec::new());
    }

    #[test]
    fn compatibility_guard_stack_dedupes_projected_guards() {
        let predicate = Predicate::from(Guard::Truthy {
            path: "enabled".to_string(),
        });

        assert_eq!(
            Predicate::compatibility_guard_stack(&[predicate.clone(), predicate]),
            vec![Guard::Truthy {
                path: "enabled".to_string()
            }]
        );
    }
}
