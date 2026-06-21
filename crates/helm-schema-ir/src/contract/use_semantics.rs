use std::collections::BTreeSet;

use crate::Guard;
use crate::contract_signals::ConditionalGuard;
use crate::predicate::Predicate;

use super::ContractUse;

impl ContractUse {
    pub(crate) fn predicate_stack(&self) -> Vec<Predicate> {
        self.guards.iter().cloned().map(Predicate::from).collect()
    }

    pub(crate) fn guard_value_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        for predicate in self.predicate_stack() {
            collect_predicate_value_paths(&predicate, &mut paths);
        }
        paths
    }

    pub(crate) fn top_level_range_guard_paths(&self) -> BTreeSet<String> {
        self.predicate_stack()
            .into_iter()
            .filter_map(|predicate| match predicate {
                Predicate::Guard(Guard::Range { path }) => Some(path),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn conditional_guard_predicates(&self) -> Vec<ConditionalGuard> {
        let mut predicates = self
            .predicate_stack()
            .into_iter()
            .filter_map(|predicate| predicate_to_conditional_guard(&predicate))
            .collect::<Vec<_>>();
        predicates.sort();
        predicates.dedup();
        predicates
    }

    pub(crate) fn lowerable_conditional_guard_set(&self) -> Option<Vec<ConditionalGuard>> {
        if path_contains_wildcard(&self.source_expr) {
            return None;
        }

        let mut guards = Vec::new();
        for predicate in self.predicate_stack() {
            extend_lowerable_predicate(&predicate, &self.source_expr, &mut guards)?;
        }
        guards.sort();
        guards.dedup();
        Some(guards)
    }

    pub(crate) fn conditionally_optional_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        for predicate in self.predicate_stack() {
            collect_conditionally_optional_paths(&predicate, &mut paths);
        }
        paths
    }

    pub(crate) fn default_fallback_paths(&self) -> BTreeSet<String> {
        self.predicate_stack()
            .into_iter()
            .filter_map(|predicate| match predicate {
                Predicate::Guard(Guard::Default { path }) => Some(path),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn has_matching_self_guard(&self) -> bool {
        self.predicate_stack()
            .into_iter()
            .any(|predicate| predicate_is_self_guarding(&predicate, &self.source_expr))
    }

    pub(crate) fn has_self_range_guard(&self) -> bool {
        self.predicate_stack()
            .into_iter()
            .any(|predicate| matches!(predicate, Predicate::Guard(Guard::Range { path }) if path == self.source_expr))
    }

    pub(crate) fn has_pathless_self_default_guard(&self) -> bool {
        self.path.0.is_empty()
            && self
                .predicate_stack()
                .into_iter()
                .any(|predicate| matches!(predicate, Predicate::Guard(Guard::Default { path }) if path == self.source_expr))
    }

    pub(crate) fn is_positive_header(&self) -> bool {
        let predicates = self.predicate_stack();
        !predicates.is_empty()
            && predicates
                .iter()
                .all(|predicate| predicate_is_positive_header(predicate, &self.source_expr))
    }
}

fn collect_predicate_value_paths(predicate: &Predicate, out: &mut BTreeSet<String>) {
    match predicate {
        Predicate::True | Predicate::False => {}
        Predicate::Guard(guard) => {
            for path in guard.value_paths() {
                out.insert(path.to_string());
            }
        }
        Predicate::Not(inner) => collect_predicate_value_paths(inner, out),
        Predicate::And(predicates) | Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_predicate_value_paths(predicate, out);
            }
        }
    }
}

fn predicate_to_conditional_guard(predicate: &Predicate) -> Option<ConditionalGuard> {
    match predicate {
        Predicate::True | Predicate::False => None,
        Predicate::Guard(Guard::Truthy { path }) => {
            Some(ConditionalGuard::Truthy { path: path.clone() })
        }
        Predicate::Guard(Guard::With { path }) => {
            Some(ConditionalGuard::With { path: path.clone() })
        }
        Predicate::Guard(Guard::Eq { path, value }) => Some(ConditionalGuard::Eq {
            path: path.clone(),
            value: value.clone(),
        }),
        Predicate::Guard(Guard::NotEq { path, value }) => Some(ConditionalGuard::NotEq {
            path: path.clone(),
            value: value.clone(),
        }),
        Predicate::Guard(Guard::Absent { path }) => {
            Some(ConditionalGuard::Absent { path: path.clone() })
        }
        Predicate::Guard(Guard::TypeIs { path, schema_type }) => Some(ConditionalGuard::TypeIs {
            path: path.clone(),
            schema_type: schema_type.clone(),
        }),
        Predicate::Not(inner) => Some(ConditionalGuard::Not(Box::new(
            predicate_to_conditional_guard(inner)?,
        ))),
        Predicate::And(predicates) => {
            let mut guards = predicates
                .iter()
                .map(predicate_to_conditional_guard)
                .collect::<Option<Vec<_>>>()?;
            guards.sort();
            guards.dedup();
            match guards.as_slice() {
                [] => None,
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AllOf(guards)),
            }
        }
        Predicate::Or(predicates) => {
            let mut guards = predicates
                .iter()
                .map(predicate_to_conditional_guard)
                .collect::<Option<Vec<_>>>()?;
            guards.sort();
            guards.dedup();
            (!guards.is_empty()).then_some(ConditionalGuard::AnyOf(guards))
        }
        Predicate::Guard(Guard::Range { .. } | Guard::Default { .. }) => None,
        Predicate::Guard(Guard::Not { .. } | Guard::Or { .. } | Guard::AnyOf { .. }) => None,
    }
}

fn extend_lowerable_predicate(
    predicate: &Predicate,
    target_value_path: &str,
    out: &mut Vec<ConditionalGuard>,
) -> Option<()> {
    match predicate {
        Predicate::True | Predicate::False => return None,
        Predicate::Guard(Guard::With { .. }) => {}
        Predicate::Guard(Guard::Truthy { path }) => {
            out.push(ConditionalGuard::Truthy {
                path: lowerable_guard_path(path, target_value_path)?,
            });
        }
        Predicate::Guard(Guard::Eq { path, value }) => {
            out.push(ConditionalGuard::Eq {
                path: lowerable_guard_path(path, target_value_path)?,
                value: value.clone(),
            });
        }
        Predicate::Guard(Guard::NotEq { path, value }) => {
            out.push(ConditionalGuard::NotEq {
                path: lowerable_guard_path(path, target_value_path)?,
                value: value.clone(),
            });
        }
        Predicate::Guard(Guard::Absent { path }) => {
            out.push(ConditionalGuard::Absent {
                path: lowerable_guard_path(path, target_value_path)?,
            });
        }
        Predicate::Guard(Guard::TypeIs { path, schema_type }) => {
            out.push(ConditionalGuard::TypeIs {
                path: lowerable_guard_path(path, target_value_path)?,
                schema_type: schema_type.clone(),
            });
        }
        Predicate::Not(inner) => {
            out.push(ConditionalGuard::Not(Box::new(lowerable_single_predicate(
                inner,
                target_value_path,
            )?)));
        }
        Predicate::And(predicates) => {
            for predicate in predicates {
                extend_lowerable_predicate(predicate, target_value_path, out)?;
            }
        }
        Predicate::Or(predicates) => {
            let mut guards = predicates
                .iter()
                .map(|predicate| lowerable_single_predicate(predicate, target_value_path))
                .collect::<Option<Vec<_>>>()?;
            guards.sort();
            guards.dedup();
            out.push(ConditionalGuard::AnyOf(guards));
        }
        Predicate::Guard(Guard::Range { .. }) => return None,
        Predicate::Guard(Guard::Default { path }) if path == target_value_path => {}
        Predicate::Guard(Guard::Default { .. }) => return None,
        Predicate::Guard(Guard::Not { .. } | Guard::Or { .. } | Guard::AnyOf { .. }) => {
            return None;
        }
    }
    Some(())
}

fn lowerable_single_predicate(
    predicate: &Predicate,
    target_value_path: &str,
) -> Option<ConditionalGuard> {
    match predicate {
        Predicate::And(predicates) => {
            let mut guards = predicates
                .iter()
                .map(|predicate| lowerable_single_predicate(predicate, target_value_path))
                .collect::<Option<Vec<_>>>()?;
            guards.sort();
            guards.dedup();
            match guards.as_slice() {
                [] => None,
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AllOf(guards)),
            }
        }
        other => {
            let mut guards = Vec::new();
            extend_lowerable_predicate(other, target_value_path, &mut guards)?;
            match guards.as_slice() {
                [] => None,
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AllOf(guards)),
            }
        }
    }
}

fn collect_conditionally_optional_paths(predicate: &Predicate, out: &mut BTreeSet<String>) {
    match predicate {
        Predicate::Guard(Guard::NotEq { path, .. } | Guard::Absent { path }) => {
            out.insert(path.clone());
        }
        Predicate::Not(inner) => match inner.as_ref() {
            Predicate::Guard(Guard::Truthy { path }) => {
                out.insert(path.clone());
            }
            _ => collect_conditionally_optional_paths(inner, out),
        },
        Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_predicate_value_paths(predicate, out);
            }
        }
        Predicate::And(predicates) => {
            for predicate in predicates {
                collect_conditionally_optional_paths(predicate, out);
            }
        }
        Predicate::True
        | Predicate::False
        | Predicate::Guard(
            Guard::Truthy { .. }
            | Guard::Eq { .. }
            | Guard::Range { .. }
            | Guard::With { .. }
            | Guard::Default { .. }
            | Guard::TypeIs { .. }
            | Guard::Not { .. }
            | Guard::Or { .. }
            | Guard::AnyOf { .. },
        ) => {}
    }
}

fn predicate_is_self_guarding(predicate: &Predicate, source_expr: &str) -> bool {
    matches!(
        predicate,
        Predicate::Guard(
            Guard::Truthy { path }
                | Guard::Eq { path, .. }
                | Guard::Range { path }
                | Guard::With { path }
                | Guard::Default { path }
        ) if path == source_expr
    )
}

fn predicate_is_positive_header(predicate: &Predicate, source_expr: &str) -> bool {
    matches!(
        predicate,
        Predicate::Guard(Guard::Truthy { path }
            | Guard::Eq { path, .. }
            | Guard::TypeIs { path, .. }) if path == source_expr
    )
}

fn lowerable_guard_path(path: &str, target_value_path: &str) -> Option<String> {
    (!path_contains_wildcard(path) && path != target_value_path).then(|| path.to_string())
}

fn path_contains_wildcard(path: &str) -> bool {
    path.split('.').any(|segment| segment == "*")
}
