use std::collections::{BTreeMap, BTreeSet};

use crate::Guard;
use crate::ProviderSchemaUse;
use crate::ValueKind;
use crate::contract_signals::{
    ConditionalGuard, ContractRequirednessEvidence, ContractValuePathFacts, MetadataFieldKind,
};
use crate::predicate::Predicate;

use super::ContractUse;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContractPathObservation {
    pub(crate) referenced: bool,
    pub(crate) guard_predicates: Vec<ConditionalGuard>,
    pub(crate) requiredness: ContractRequirednessEvidence,
    pub(crate) facts: ContractValuePathFacts,
    pub(crate) type_hints: BTreeSet<String>,
    pub(crate) metadata_field_kind: Option<MetadataFieldKind>,
    pub(crate) source_null_tolerant: Option<bool>,
    pub(crate) source_lowerable_conditional_guards: Option<Vec<ConditionalGuard>>,
    pub(crate) provider_schema_use: Option<ProviderSchemaUse>,
}

impl ContractPathObservation {
    pub(crate) fn type_hint(
        value_path: &str,
        schema_types: &BTreeSet<String>,
    ) -> Option<(String, Self)> {
        if value_path.trim().is_empty() {
            return None;
        }
        let schema_types = schema_types
            .iter()
            .filter(|schema_type| !schema_type.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if schema_types.is_empty() {
            return None;
        }

        let mut observation = Self {
            referenced: true,
            ..Self::default()
        };
        observation.type_hints = schema_types;
        Some((value_path.to_string(), observation))
    }

    pub(crate) fn dependency_values_root_fragment(value_path: &str) -> Option<(String, Self)> {
        if value_path.trim().is_empty() {
            return None;
        }

        Some((
            value_path.to_string(),
            Self {
                referenced: true,
                facts: ContractValuePathFacts {
                    accepted_values_root_fragment: true,
                    accepted_dependency_values_root_fragment: true,
                    ..ContractValuePathFacts::default()
                },
                ..Self::default()
            },
        ))
    }
}

pub(crate) fn contract_path_observations(
    contract_use: &ContractUse,
) -> BTreeMap<String, ContractPathObservation> {
    let predicates = predicate_stack(contract_use);
    let has_source = !contract_use.source_expr.trim().is_empty();
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
    let range_guard_paths: BTreeSet<String> = predicates
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::Range { path }) => Some(path.clone()),
            _ => None,
        })
        .collect();
    let mut conditional_guard_predicates = predicates
        .iter()
        .filter_map(predicate_to_conditional_guard)
        .collect::<Vec<_>>();
    conditional_guard_predicates.sort();
    conditional_guard_predicates.dedup();
    let positive_header = contract_use.kind == ValueKind::Scalar
        && path_is_empty
        && !predicates.is_empty()
        && predicates
            .iter()
            .all(|predicate| predicate_is_positive_header(predicate, &contract_use.source_expr));
    let mut path_observations = BTreeMap::new();

    if has_source
        && let Some(observation) =
            path_observation(&mut path_observations, &contract_use.source_expr)
    {
        observation.metadata_field_kind = metadata_field_kind_from_yaml_path(&contract_use.path.0);
        observation.facts.used_as_fragment = contract_use.kind == ValueKind::Fragment;
        observation.facts.used_as_pathless_fragment =
            contract_use.kind == ValueKind::Fragment && contract_use.path.0.is_empty();
        observation.facts.is_partial_scalar_value_path =
            contract_use.kind == ValueKind::PartialScalar && !contract_use.path.0.is_empty();
        if !path_is_empty {
            observation
                .facts
                .record_render_use(self_range_guarded, Some(has_matching_self_guard));
            observation.facts.has_unconditional_render_use = contract_use.guards.is_empty();
        }
        observation.source_null_tolerant = Some(path_is_empty || has_matching_self_guard);
        observation.source_lowerable_conditional_guards =
            lowerable_conditional_guard_set(contract_use, &predicates);
        observation.provider_schema_use = provider_schema_use(contract_use, self_range_guarded);
        observation.requiredness.is_positive_header = positive_header;
        observation.facts.is_nullable |= !path_is_empty
            || self_range_guarded
            || contract_use.kind == ValueKind::Fragment
            || pathless_self_default_guarded;
    }

    for path in predicates
        .iter()
        .flat_map(Predicate::conditionally_optional_paths)
    {
        if let Some(observation) = path_observation(&mut path_observations, &path) {
            observation.requiredness.is_conditionally_optional = true;
        }
    }
    for path in predicates.iter().filter_map(|predicate| match predicate {
        Predicate::Guard(Guard::Default { path }) => Some(path),
        _ => None,
    }) {
        if let Some(observation) = path_observation(&mut path_observations, path) {
            observation.requiredness.has_default_fallback = true;
        }
    }
    if has_source {
        for predicate in conditional_guard_predicates {
            for path in predicate.value_paths() {
                if let Some(observation) = path_observation(&mut path_observations, &path)
                    && !observation.guard_predicates.contains(&predicate)
                {
                    observation.guard_predicates.push(predicate.clone());
                }
            }
        }
    }
    for path in predicates.iter().flat_map(Predicate::value_paths) {
        if has_source && path == contract_use.source_expr.as_str() {
            continue;
        }
        let Some(observation) = path_observation(&mut path_observations, &path) else {
            continue;
        };
        observation.referenced |= has_source;
        if !path_is_empty {
            observation
                .facts
                .record_render_use(range_guard_paths.contains(&path), None);
        }
    }
    if has_source {
        for path in range_guard_paths {
            if let Some(observation) = path_observation(&mut path_observations, &path) {
                observation.facts.is_ranged_source = true;
                observation.facts.is_nullable = true;
            }
        }
    }

    path_observations
}

fn metadata_field_kind_from_yaml_path(path: &[String]) -> Option<MetadataFieldKind> {
    if path.get(path.len().checked_sub(2)?)?.as_str() != "metadata" {
        return None;
    }

    match path.last()?.as_str() {
        "labels" | "annotations" => Some(MetadataFieldKind::StringMap),
        "name" => Some(MetadataFieldKind::Name),
        "namespace" => Some(MetadataFieldKind::Namespace),
        _ => None,
    }
}

fn path_observation<'a>(
    observations: &'a mut BTreeMap<String, ContractPathObservation>,
    path: &str,
) -> Option<&'a mut ContractPathObservation> {
    (!path.trim().is_empty()).then(|| observations.entry(path.to_string()).or_default())
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
