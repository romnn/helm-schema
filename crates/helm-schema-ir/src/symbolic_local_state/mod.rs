use std::collections::BTreeSet;

use crate::scalar_value::TruthCondition;
use crate::value::State;
use helm_schema_core::{Predicate, PredicateMemo};

/// The values path (and optional literal-integer `default` fallback) behind
/// a local's `int`/`int64` cast binding. The fallback matters for
/// soundness: a raw `0` at the path is numerically empty, so `default`
/// substitutes the literal before the comparison runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IntCastSource {
    pub(crate) path: helm_schema_core::ValuesPath,
    pub(crate) default_int: Option<i64>,
}

/// One complete local-state exit selected by a structural control condition.
#[derive(Clone, Debug)]
pub(crate) struct ControlOutcome {
    condition: Predicate,
    pub(crate) truth: Option<TruthCondition>,
    pub(crate) state: State,
}

impl ControlOutcome {
    pub(crate) fn new(truth: TruthCondition, state: State) -> Self {
        let condition = truth
            .predicate()
            .cloned()
            .unwrap_or_else(|| truth.when_true());
        Self {
            condition,
            truth: Some(truth),
            state,
        }
    }

    pub(crate) fn unresolved_from_changed(entry: &State, known: &[Self]) -> Self {
        let variables = known
            .iter()
            .flat_map(|outcome| {
                outcome
                    .state
                    .iter_locals()
                    .filter_map(|(name, slot)| slot.binding().binding.is_some().then_some(name))
            })
            .chain(
                entry
                    .iter_locals()
                    .filter_map(|(name, slot)| slot.binding().binding.is_some().then_some(name)),
            )
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut state = entry.clone();
        for variable in variables {
            if known.iter().all(|outcome| {
                outcome.state.read_local(&variable).binding.as_ref()
                    == entry.read_local(&variable).binding.as_ref()
            }) {
                continue;
            }
            state.invalidate_binding(&variable);
        }
        Self {
            condition: Predicate::False,
            truth: None,
            state,
        }
    }

    pub(crate) fn condition(&self) -> Predicate {
        self.condition.clone()
    }

    pub(crate) fn guard_all(&mut self, condition: Predicate, memo: &PredicateMemo) {
        self.condition = crate::scalar_value::conjoin_predicates_with_memo(
            condition.clone(),
            self.condition.clone(),
            memo,
        )
        .unwrap_or(Predicate::False);
        let Some(truth) = self.truth.take() else {
            return;
        };
        self.truth = Some(TruthCondition::all_with_memo(
            [
                TruthCondition::from_predicate_with_memo(condition, memo),
                truth,
            ],
            memo,
        ));
    }

    pub(crate) fn replace_state(&mut self, state: &State) {
        self.state = state.fork_locals();
    }
}
