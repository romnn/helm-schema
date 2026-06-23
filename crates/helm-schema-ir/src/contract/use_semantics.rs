use std::collections::BTreeSet;

use crate::Guard;
use crate::ProviderSchemaUse;
use crate::ValueKind;
use crate::contract_signals::ConditionalGuard;
use crate::predicate::Predicate;

use super::ContractUse;

pub(crate) struct ContractUseObservation {
    pub(crate) has_source: bool,
    pub(crate) has_render_path: bool,
    pub(crate) range_guard_paths: BTreeSet<String>,
    pub(crate) guard_value_paths: BTreeSet<String>,
    pub(crate) conditional_guard_predicates: Vec<ConditionalGuard>,
    pub(crate) lowerable_conditional_guards: Option<Vec<ConditionalGuard>>,
    pub(crate) conditionally_optional_paths: BTreeSet<String>,
    pub(crate) default_fallback_paths: BTreeSet<String>,
    pub(crate) provider_schema_use: Option<ProviderSchemaUse>,
    pub(crate) self_guarded: bool,
    pub(crate) self_range_guarded: bool,
    pub(crate) pathless_self_default_guarded: bool,
    pub(crate) null_tolerant: bool,
    pub(crate) positive_header: bool,
}

impl ContractUseObservation {
    pub(crate) fn new(contract_use: &ContractUse) -> Self {
        let predicates = predicate_stack(contract_use);
        let has_source = !contract_use.source_expr.trim().is_empty();
        let has_render_path = !contract_use.path.0.is_empty();
        let path_is_empty = contract_use.path.0.is_empty();
        let self_range_guarded = predicates.iter().any(|predicate| {
            matches!(predicate, Predicate::Guard(Guard::Range { path }) if path == &contract_use.source_expr)
        });
        let has_matching_self_guard = predicates
            .iter()
            .any(|predicate| predicate_is_self_guarding(predicate, &contract_use.source_expr));
        let pathless_self_default_guarded = path_is_empty
            && predicates.iter().any(|predicate| {
                matches!(predicate, Predicate::Guard(Guard::Default { path }) if path == &contract_use.source_expr)
            });
        let range_guard_paths = predicates
            .iter()
            .filter_map(|predicate| match predicate {
                Predicate::Guard(Guard::Range { path }) => Some(path.clone()),
                _ => None,
            })
            .collect();
        let guard_value_paths = predicates.iter().flat_map(Predicate::value_paths).collect();
        let mut conditional_guard_predicates = predicates
            .iter()
            .filter_map(predicate_to_conditional_guard)
            .collect::<Vec<_>>();
        conditional_guard_predicates.sort();
        conditional_guard_predicates.dedup();
        let conditionally_optional_paths = predicates
            .iter()
            .flat_map(Predicate::conditionally_optional_paths)
            .collect();
        let default_fallback_paths = predicates
            .iter()
            .filter_map(|predicate| match predicate {
                Predicate::Guard(Guard::Default { path }) => Some(path.clone()),
                _ => None,
            })
            .collect();
        let positive_header = contract_use.kind == ValueKind::Scalar
            && path_is_empty
            && !predicates.is_empty()
            && predicates.iter().all(|predicate| {
                predicate_is_positive_header(predicate, &contract_use.source_expr)
            });

        Self {
            has_source,
            has_render_path,
            range_guard_paths,
            guard_value_paths,
            conditional_guard_predicates,
            lowerable_conditional_guards: lowerable_conditional_guard_set(
                contract_use,
                &predicates,
            ),
            conditionally_optional_paths,
            default_fallback_paths,
            provider_schema_use: provider_schema_use(contract_use, self_range_guarded),
            self_guarded: has_source && (path_is_empty || has_matching_self_guard),
            self_range_guarded,
            pathless_self_default_guarded,
            null_tolerant: !has_source || path_is_empty || has_matching_self_guard,
            positive_header,
        }
    }
}

fn predicate_stack(contract_use: &ContractUse) -> Vec<Predicate> {
    contract_use
        .guards
        .iter()
        .cloned()
        .map(Predicate::from)
        .collect()
}

fn lowerable_conditional_guard_set(
    contract_use: &ContractUse,
    predicates: &[Predicate],
) -> Option<Vec<ConditionalGuard>> {
    if path_contains_wildcard(&contract_use.source_expr) {
        return None;
    }

    let mut guards = Vec::new();
    for predicate in predicates {
        extend_lowerable_predicate(predicate, &contract_use.source_expr, &mut guards)?;
    }
    guards.sort();
    guards.dedup();
    Some(guards)
}

fn provider_schema_use(
    contract_use: &ContractUse,
    self_range_guarded: bool,
) -> Option<ProviderSchemaUse> {
    if contract_use.source_expr.trim().is_empty()
        || contract_use.kind == ValueKind::PartialScalar
        || contract_use.path.0.is_empty()
    {
        return None;
    }
    let resource = contract_use.resource.clone()?;

    Some(ProviderSchemaUse {
        value_path: contract_use.source_expr.clone(),
        path: contract_use.path.clone(),
        kind: contract_use.kind,
        resource,
        is_self_range_collection: self_range_guarded
            && contract_use
                .path
                .0
                .last()
                .is_none_or(|segment| !segment.ends_with("[*]")),
    })
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
