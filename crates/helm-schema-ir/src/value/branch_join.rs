use std::collections::BTreeSet;

use crate::eval_env::{BindingDecision, LocalBinding};
use crate::scalar_value::{
    ScalarValueDispatch, TruthCondition, any_predicates_with_memo, conjoin_predicates_with_memo,
};
use helm_schema_core::{Predicate, PredicateMemo};

use crate::symbolic_local_state::SymbolicLocalState;
use crate::value::LocalSlot;

const MAX_JOINED_SCALAR_ARMS: usize = 128;

pub(super) fn joined_branch_outcomes(
    entry: &SymbolicLocalState,
    outcomes: &[SymbolicLocalState],
) -> SymbolicLocalState {
    if outcomes.is_empty() {
        return entry.clone();
    }

    let variables: BTreeSet<&String> = outcomes
        .iter()
        .flat_map(|state| state.slots.keys())
        .collect();
    let slots = variables
        .into_iter()
        .map(|name| {
            let exits = outcomes
                .iter()
                .map(|state| state.slot(name))
                .collect::<Vec<_>>();
            (name.clone(), LocalSlot::join(&exits))
        })
        .collect();
    SymbolicLocalState {
        slots,
        chart_value_defaults: intersect_chart_defaults(outcomes),
        scopes: entry.scopes.clone(),
    }
}

impl SymbolicLocalState {
    /// Rebuild scalar locals from the mutually exclusive arms of one exact
    /// `if` chain. Ordinary local facts join by equality or value choice;
    /// scalar values retain the condition that selected each reassignment.
    pub(crate) fn join_scalar_dispatch_arms(
        &mut self,
        entry: &SymbolicLocalState,
        arms: &[(TruthCondition, SymbolicLocalState)],
        has_unconditional_else: bool,
        memo: &PredicateMemo,
    ) {
        if arms
            .iter()
            .any(|(condition, _)| matches!(condition, TruthCondition::Unknown))
        {
            return;
        }
        let mut outcomes = arms.to_vec();
        if !has_unconditional_else {
            outcomes.push((
                TruthCondition::any_with_memo(
                    arms.iter().map(|(condition, _)| condition.clone()),
                    memo,
                )
                .negated_with_memo(memo),
                entry.clone(),
            ));
        }

        let variables: BTreeSet<&String> = arms
            .iter()
            .flat_map(|(_, state)| {
                state
                    .slots
                    .iter()
                    .filter_map(|(name, slot)| slot.scalar_dispatch.is_some().then_some(name))
            })
            .chain(
                entry
                    .slots
                    .iter()
                    .filter_map(|(name, slot)| slot.scalar_dispatch.is_some().then_some(name)),
            )
            .collect();
        for slot in self.slots.values_mut() {
            slot.scalar_dispatch = None;
        }
        for variable in variables {
            if let Some(entry_dispatch) = entry.slot(variable).scalar_dispatch.as_ref()
                && outcomes.iter().all(|(_, state)| {
                    state.slot(variable).scalar_dispatch.as_ref() == Some(entry_dispatch)
                })
            {
                self.slot_mut(variable).scalar_dispatch = Some(entry_dispatch.clone());
                continue;
            }
            let mut dispatch_arms = Vec::new();
            let mut complete = outcomes
                .iter()
                .all(|(condition, _)| condition.predicate().is_some());
            'outcomes: for (condition, state) in &outcomes {
                let outer_condition = condition.when_true();
                if outer_condition == Predicate::False {
                    continue;
                }
                let Some(dispatch) = state.slot(variable).scalar_dispatch.as_ref() else {
                    complete = false;
                    continue;
                };
                complete &= dispatch.complete;
                for (inner_condition, value) in &dispatch.arms {
                    if let Some(condition) = conjoin_predicates_with_memo(
                        outer_condition.clone(),
                        inner_condition.clone(),
                        memo,
                    ) {
                        dispatch_arms.push((condition, value.clone()));
                        if dispatch_arms.len() > MAX_JOINED_SCALAR_ARMS {
                            break 'outcomes;
                        }
                    }
                }
            }
            if dispatch_arms.is_empty() || dispatch_arms.len() > MAX_JOINED_SCALAR_ARMS {
                continue;
            }
            self.slot_mut(variable).scalar_dispatch = Some(ScalarValueDispatch {
                arms: dispatch_arms,
                complete,
            });
        }
    }

    pub(crate) fn join_binding_decisions(
        &mut self,
        fallthrough: &SymbolicLocalState,
        decisions: &[Option<std::rc::Rc<BindingDecision>>],
        outcomes: &[SymbolicLocalState],
    ) {
        let fallthrough = outcomes.get(decisions.len()).unwrap_or(fallthrough);
        let variables: BTreeSet<&String> = outcomes
            .iter()
            .flat_map(|state| {
                state
                    .slots
                    .iter()
                    .filter_map(|(name, slot)| slot.binding.is_some().then_some(name))
            })
            .chain(
                fallthrough
                    .slots
                    .iter()
                    .filter_map(|(name, slot)| slot.binding.is_some().then_some(name)),
            )
            .collect();
        for slot in self.slots.values_mut() {
            slot.binding = None;
        }
        for variable in variables {
            let mut binding = fallthrough
                .slot(variable)
                .binding
                .clone()
                .unwrap_or_else(LocalBinding::unknown);
            for (decision, outcome) in decisions.iter().zip(outcomes).rev() {
                let outcome = outcome
                    .slot(variable)
                    .binding
                    .clone()
                    .unwrap_or_else(LocalBinding::unknown);
                binding = match decision {
                    Some(decision) => {
                        LocalBinding::select(std::rc::Rc::clone(decision), outcome, binding)
                    }
                    None => outcome,
                };
            }
            self.slot_mut(variable).binding = Some(binding);
        }
    }

    /// Rebuild local truthiness from the mutually exclusive arms of one
    /// exact `if` chain. Every arm, including the implicit fallthrough,
    /// contributes its selection condition so an untouched entry reduction
    /// cannot erase a kill-switch reassignment at the union.
    pub(crate) fn join_truthy_reduction_arms(
        &mut self,
        entry: &SymbolicLocalState,
        arms: &[(TruthCondition, SymbolicLocalState)],
        has_unconditional_else: bool,
        memo: &PredicateMemo,
    ) {
        let mut outcomes = arms.to_vec();
        if !has_unconditional_else {
            outcomes.push((
                TruthCondition::any_with_memo(
                    arms.iter().map(|(condition, _)| condition.clone()),
                    memo,
                )
                .negated_with_memo(memo),
                entry.clone(),
            ));
        }
        if outcomes
            .iter()
            .any(|(condition, _)| condition.predicate().is_none())
        {
            return;
        }

        let variables: BTreeSet<&String> = entry
            .slots
            .iter()
            .filter_map(|(name, slot)| slot.truthy_reduction.is_some().then_some(name))
            .collect();
        for variable in variables {
            let Some(entry_reduction) = entry.slot(variable).truthy_reduction.as_ref() else {
                continue;
            };
            if matches!(
                entry_reduction.kind(),
                helm_schema_core::PredicateKind::False
            ) || !outcomes.iter().any(|(_, state)| {
                matches!(
                    state.slot(variable).truthy_reduction.as_ref(),
                    Some(predicate) if predicate == &Predicate::False
                )
            }) {
                continue;
            }
            let mut alternatives = Vec::new();
            let mut complete = true;
            for (condition, state) in &outcomes {
                let arm_condition = condition.when_true();
                let Some(reduction) = state.slot(variable).truthy_reduction.as_ref() else {
                    complete = false;
                    break;
                };
                if reduction.contains_approximation() {
                    complete = false;
                    break;
                }
                if let Some(alternative) =
                    conjoin_predicates_with_memo(arm_condition, reduction.clone(), memo)
                {
                    alternatives.push(alternative);
                }
            }
            if complete {
                self.slot_mut(variable).truthy_reduction =
                    Some(any_predicates_with_memo(alternatives, memo));
            }
        }
    }
}

fn intersect_chart_defaults(
    outcomes: &[SymbolicLocalState],
) -> BTreeSet<helm_schema_core::ValuesPath> {
    let Some((first, rest)) = outcomes.split_first() else {
        return BTreeSet::new();
    };
    let mut defaults = first.chart_value_defaults.clone();
    for outcome in rest {
        defaults.retain(|path| outcome.chart_value_defaults.contains(path));
    }
    defaults
}
