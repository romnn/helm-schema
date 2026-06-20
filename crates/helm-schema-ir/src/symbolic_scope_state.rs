use crate::Guard;
use crate::abstract_value::AbstractValue;
use crate::predicate::Predicate;
use crate::symbolic_local_state::{SymbolicLocalState, SymbolicLocalStateSnapshot};

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolicScopeState {
    predicates: Vec<Predicate>,
    dot_stack: Vec<Option<AbstractValue>>,
    locals: SymbolicLocalState,
}

#[derive(Clone, Debug)]
pub(crate) struct SymbolicScopeSnapshot {
    predicates_len: usize,
    dot_stack_len: usize,
    locals: SymbolicLocalStateSnapshot,
}

impl SymbolicScopeState {
    pub(crate) fn reset_control(&mut self, predicates: &[Predicate], dot: Option<AbstractValue>) {
        self.predicates = predicates.to_vec();
        self.dot_stack.clear();
        if let Some(dot) = dot {
            self.dot_stack.push(Some(dot));
        }
    }

    pub(crate) fn snapshot(&self) -> SymbolicScopeSnapshot {
        SymbolicScopeSnapshot {
            predicates_len: self.predicates.len(),
            dot_stack_len: self.dot_stack.len(),
            locals: self.locals.snapshot(),
        }
    }

    pub(crate) fn restore(&mut self, snapshot: SymbolicScopeSnapshot) {
        self.predicates.truncate(snapshot.predicates_len);
        self.dot_stack.truncate(snapshot.dot_stack_len);
        self.locals.restore(snapshot.locals);
    }

    pub(crate) fn join_branch_outcomes(
        &mut self,
        entry: &SymbolicScopeSnapshot,
        outcomes: Vec<SymbolicScopeSnapshot>,
    ) {
        self.predicates.truncate(entry.predicates_len);
        self.dot_stack.truncate(entry.dot_stack_len);
        self.locals.join_branch_outcomes(
            &entry.locals,
            outcomes
                .into_iter()
                .map(|snapshot| snapshot.locals)
                .collect(),
        );
    }

    pub(crate) fn contract_guards(&self) -> Vec<Guard> {
        Predicate::contract_guard_stack(&self.predicates)
    }

    pub(crate) fn predicates(&self) -> &[Predicate] {
        &self.predicates
    }

    pub(crate) fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() && !self.predicates.contains(&predicate) {
            self.predicates.push(predicate);
        }
    }

    pub(crate) fn push_dot_binding(&mut self, binding: Option<AbstractValue>) {
        self.dot_stack.push(binding);
    }

    pub(crate) fn current_dot_binding(&self) -> Option<AbstractValue> {
        self.dot_stack
            .last()
            .and_then(|binding| binding.as_ref())
            .and_then(AbstractValue::to_current_dot_context_value)
    }

    pub(crate) fn current_dot_fragment(&self) -> Option<AbstractValue> {
        self.dot_stack.last().cloned().flatten()
    }

    pub(crate) fn locals(&self) -> &SymbolicLocalState {
        &self.locals
    }

    pub(crate) fn locals_mut(&mut self) -> &mut SymbolicLocalState {
        &mut self.locals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn branch_join_restores_control_state_to_entry() {
        let mut state = SymbolicScopeState::default();
        state.push_predicate_if_absent(Predicate::truthy_path("enabled"));
        state.push_dot_binding(Some(AbstractValue::ValuesPath("root".to_string())));
        let entry = state.snapshot();

        state.push_predicate_if_absent(Predicate::truthy_path("branch"));
        state.push_dot_binding(Some(AbstractValue::ValuesPath("branch".to_string())));
        let branch = state.snapshot();

        state.restore(entry.clone());
        state.join_branch_outcomes(&entry, vec![branch]);

        sim_assert_eq!(
            have: state.contract_guards(),
            want: vec![Guard::Truthy {
                path: "enabled".to_string()
            }]
        );
        sim_assert_eq!(
            have: state.current_dot_fragment(),
            want: Some(AbstractValue::ValuesPath("root".to_string()))
        );
    }

    #[test]
    fn branch_join_still_joins_local_state() {
        let mut state = SymbolicScopeState::default();
        let entry = state.snapshot();

        state.locals_mut().insert_range_domain(
            "scope".to_string(),
            vec!["frontend".to_string(), "backend".to_string()],
        );
        let branch = state.snapshot();

        state.restore(entry.clone());
        state
            .locals_mut()
            .insert_range_domain("scope".to_string(), vec!["frontend".to_string()]);
        let other_branch = state.snapshot();

        state.restore(entry.clone());
        state.join_branch_outcomes(&entry, vec![branch, other_branch]);

        sim_assert_eq!(
            have: state.locals().range_domains.get("scope"),
            want: Some(&vec!["backend".to_string(), "frontend".to_string()])
        );
    }
}
