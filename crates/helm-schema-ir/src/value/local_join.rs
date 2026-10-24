use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::Predicate;

use crate::abstract_value::AbstractValue;
use crate::eval_env::LocalBinding;
use crate::helper_meta::HelperOutputMeta;

use super::LocalSlot;

impl LocalSlot {
    pub(crate) fn join(outcomes: &[&Self]) -> Self {
        let values = outcomes
            .iter()
            .filter_map(|slot| slot.binding.as_ref())
            .collect::<Vec<_>>();
        let binding = if values.is_empty() {
            None
        } else {
            let mut binding = LocalBinding::join_unconditioned(values);
            if outcomes.iter().any(|slot| slot.binding.is_none()) {
                binding = LocalBinding::select(
                    crate::eval_env::BindingDecision::new(
                        crate::scalar_value::TruthCondition::Unknown,
                    ),
                    binding,
                    LocalBinding::unknown(),
                );
            }
            Some(binding)
        };
        Self {
            range_domain: join_field(
                outcomes,
                |slot| slot.range_domain.as_ref(),
                join_literal_union,
            ),
            get_binding: join_equal(outcomes, |slot| &slot.get_binding),
            binding,
            default_paths: join_field(
                outcomes,
                |slot| slot.default_paths.as_ref(),
                join_path_union,
            ),
            output_meta: join_field(
                outcomes,
                |slot| slot.output_meta.as_ref(),
                join_meta_by_path,
            ),
            scalar_dispatch: join_equal(outcomes, |slot| &slot.scalar_dispatch),
            truthy_reduction: join_field(
                outcomes,
                |slot| slot.truthy_reduction.as_ref(),
                join_predicate_union,
            ),
            truthiness_abstained: outcomes.iter().any(|state| state.truthiness_abstained),
            // Any branch's falsy-capable reassignment poisons the accumulator's
            // monotonicity, so the marks union.
            truthiness_cleared: outcomes.iter().any(|state| state.truthiness_cleared),
            typeof_source: join_equal(outcomes, |slot| &slot.typeof_source),
            int_cast_source: join_equal(outcomes, |slot| &slot.int_cast_source),
            range_member_value: join_field(
                outcomes,
                |slot| slot.range_member_value.as_ref(),
                join_value_choice,
            )
            .flatten(),
            // A definite entry binding survives a join only where EVERY branch
            // kept the same one: a branch-dependent binding is no longer a
            // certainly-iterated member.
            definite_range_member_value: join_equal(outcomes, |slot| {
                &slot.definite_range_member_value
            }),
        }
    }
}

/// Join one slot field across branch outcomes.
///
/// A variable keeps a joined fact only when every branch outcome still carries
/// one (a branch that dropped or re-domained the variable drops the fact);
/// `join` says how the per-branch values combine into one.
fn join_field<T, U, F, J>(outcomes: &[&LocalSlot], field: F, join: J) -> Option<U>
where
    F: Fn(&LocalSlot) -> Option<&T>,
    J: Fn(Vec<&T>) -> U,
{
    let values = outcomes
        .iter()
        .map(|slot| field(slot))
        .collect::<Option<Vec<_>>>()?;
    Some(join(values))
}

fn join_equal<T: Clone + Eq>(
    outcomes: &[&LocalSlot],
    field: impl Fn(&LocalSlot) -> &Option<T>,
) -> Option<T> {
    let (first, rest) = outcomes.split_first()?;
    let first = field(first).as_ref()?;
    rest.iter()
        .all(|slot| field(slot).as_ref() == Some(first))
        .then(|| first.clone())
}

fn join_value_choice(values: Vec<&AbstractValue>) -> Option<AbstractValue> {
    AbstractValue::choice(values.into_iter().cloned().collect())
}

fn join_literal_union(domains: Vec<&Vec<String>>) -> Vec<String> {
    let literals: BTreeSet<&String> = domains.into_iter().flatten().collect();
    literals.into_iter().cloned().collect()
}

fn join_path_union(
    sets: Vec<&BTreeSet<helm_schema_core::ValuesPath>>,
) -> BTreeSet<helm_schema_core::ValuesPath> {
    sets.into_iter().flatten().cloned().collect()
}

fn join_meta_by_path(
    metas: Vec<&BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
) -> BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta> {
    let mut merged: BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta> = BTreeMap::new();
    for meta_by_path in metas {
        for (path, meta) in meta_by_path {
            merged.entry(path.clone()).or_default().merge(meta);
        }
    }
    merged
}

fn join_predicate_union(predicates: Vec<&Predicate>) -> Predicate {
    let mut alternatives = BTreeSet::new();
    for predicate in predicates {
        match predicate.kind() {
            helm_schema_core::PredicateKind::True => return Predicate::True,
            helm_schema_core::PredicateKind::False => {}
            helm_schema_core::PredicateKind::Or(inner) => {
                alternatives.extend(inner.iter().cloned());
            }
            _ => {
                alternatives.insert(predicate.clone());
            }
        }
    }
    match alternatives.len() {
        0 => Predicate::False,
        1 => alternatives.pop_first().unwrap_or(Predicate::False),
        _ => Predicate::Or(alternatives.into_iter().collect()),
    }
}
