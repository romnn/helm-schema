use std::collections::{BTreeSet, HashSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_summary::{HelperFragmentOutputUse, HelperSummary};
use crate::symbolic_local_state::SymbolicLocalState;
use helm_schema_core::Predicate;

#[derive(Clone)]
pub(crate) struct HelperRangeIteration {
    pub(crate) helper_dot_binding: Option<AbstractValue>,
    pub(crate) fragment_dot_binding: Option<AbstractValue>,
    pub(crate) variable_binding: Option<(String, AbstractValue)>,
}

#[derive(Clone)]
pub(crate) struct RangeFrame {
    pub(crate) definitely_nonempty: bool,
    pub(crate) iterations: Option<Vec<HelperRangeIteration>>,
}

#[derive(Clone)]
pub(crate) struct HelperRuntimeControlState {
    helper_dot_stack: Vec<Option<AbstractValue>>,
    fragment_dot_stack: Vec<Option<AbstractValue>>,
    active_value_predicates: BTreeSet<Predicate>,
    active_fragment_predicates: BTreeSet<Predicate>,
    active_source_relations: Vec<BTreeSet<String>>,
    range_frames: Vec<RangeFrame>,
    no_output_depth: usize,
}

#[derive(Clone)]
pub(crate) struct HelperRuntimeControlSnapshot {
    helper_dot_stack_len: usize,
    fragment_dot_stack_len: usize,
    active_value_predicates: BTreeSet<Predicate>,
    active_fragment_predicates: BTreeSet<Predicate>,
    active_source_relations: Vec<BTreeSet<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HelperRangeJoinBehavior {
    MergeAllOutcomes,
    PromoteBodyOutcome,
}

impl HelperRuntimeControlState {
    pub(crate) fn for_fragment(
        current_dot: Option<&AbstractValue>,
        current_dot_fragment: Option<&AbstractValue>,
    ) -> Self {
        Self {
            helper_dot_stack: vec![current_dot.cloned()],
            fragment_dot_stack: vec![current_dot_fragment.cloned()],
            active_value_predicates: BTreeSet::new(),
            active_fragment_predicates: BTreeSet::new(),
            active_source_relations: Vec::new(),
            range_frames: Vec::new(),
            no_output_depth: 0,
        }
    }

    pub(crate) fn current_helper_dot(&self) -> Option<&AbstractValue> {
        self.helper_dot_stack.last().and_then(Option::as_ref)
    }

    pub(crate) fn current_fragment_dot(&self) -> Option<&AbstractValue> {
        self.fragment_dot_stack.last().and_then(Option::as_ref)
    }

    pub(crate) fn active_output_predicates(&self) -> &BTreeSet<Predicate> {
        &self.active_value_predicates
    }

    pub(crate) fn active_fragment_predicates(&self) -> &BTreeSet<Predicate> {
        &self.active_fragment_predicates
    }

    pub(crate) fn active_source_relations(&self) -> &Vec<BTreeSet<String>> {
        &self.active_source_relations
    }

    pub(crate) fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() {
            self.active_value_predicates.insert(predicate.clone());
            self.active_fragment_predicates.insert(predicate);
        }
    }

    pub(crate) fn push_value_predicate_if_absent(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() {
            self.active_value_predicates.insert(predicate);
        }
    }

    pub(crate) fn extend_truthy_predicates(
        &mut self,
        guard_paths: impl IntoIterator<Item = String>,
    ) {
        for predicate in guard_paths.into_iter().map(Predicate::truthy_path) {
            self.active_value_predicates.insert(predicate.clone());
            self.active_fragment_predicates.insert(predicate);
        }
    }

    pub(crate) fn extend_source_relations(
        &mut self,
        relations: impl IntoIterator<Item = BTreeSet<String>>,
    ) {
        for relation in relations {
            if relation.len() > 1 && !self.active_source_relations.contains(&relation) {
                self.active_source_relations.push(relation);
            }
        }
    }

    pub(crate) fn push_effect_dot_binding(&mut self, binding: Option<AbstractValue>) {
        self.fragment_dot_stack.push(binding.clone());
        self.helper_dot_stack
            .push(binding.map(|binding| binding.to_context_value()));
    }

    pub(crate) fn snapshot(&self) -> HelperRuntimeControlSnapshot {
        HelperRuntimeControlSnapshot {
            helper_dot_stack_len: self.helper_dot_stack.len(),
            fragment_dot_stack_len: self.fragment_dot_stack.len(),
            active_value_predicates: self.active_value_predicates.clone(),
            active_fragment_predicates: self.active_fragment_predicates.clone(),
            active_source_relations: self.active_source_relations.clone(),
        }
    }

    pub(crate) fn restore(&mut self, snapshot: &HelperRuntimeControlSnapshot) {
        self.helper_dot_stack
            .truncate(snapshot.helper_dot_stack_len);
        self.fragment_dot_stack
            .truncate(snapshot.fragment_dot_stack_len);
        self.active_value_predicates = snapshot.active_value_predicates.clone();
        self.active_fragment_predicates = snapshot.active_fragment_predicates.clone();
        self.active_source_relations = snapshot.active_source_relations.clone();
    }

    pub(crate) fn push_range_frame(&mut self, frame: RangeFrame) {
        self.range_frames.push(frame);
    }

    pub(crate) fn prepare_branch_join(&mut self, snapshot: &HelperRuntimeControlSnapshot) {
        self.restore(snapshot);
    }

    pub(crate) fn prepare_range_join(
        &mut self,
        snapshot: &HelperRuntimeControlSnapshot,
    ) -> HelperRangeJoinBehavior {
        self.restore(snapshot);
        if self
            .range_frames
            .pop()
            .is_some_and(|frame| frame.definitely_nonempty)
        {
            HelperRangeJoinBehavior::PromoteBodyOutcome
        } else {
            HelperRangeJoinBehavior::MergeAllOutcomes
        }
    }

    pub(crate) fn range_iteration_count(&self) -> usize {
        self.range_frames
            .last()
            .and_then(|frame| frame.iterations.as_ref().map(Vec::len))
            .unwrap_or(1)
    }

    pub(crate) fn enter_range_iteration(&mut self, index: usize, locals: &mut SymbolicLocalState) {
        let Some(iteration) = self
            .range_frames
            .last()
            .and_then(|frame| frame.iterations.as_ref())
            .and_then(|iterations| iterations.get(index))
            .cloned()
        else {
            return;
        };
        if let Some((variable, binding)) = iteration.variable_binding {
            locals.fragment_values.insert(variable, binding);
        }
        self.helper_dot_stack.push(iteration.helper_dot_binding);
        self.fragment_dot_stack.push(iteration.fragment_dot_binding);
    }

    pub(crate) fn exit_range_iteration(&mut self) {
        if self
            .range_frames
            .last()
            .is_some_and(|frame| frame.iterations.is_some())
        {
            self.helper_dot_stack.pop();
            self.fragment_dot_stack.pop();
        }
    }

    pub(crate) fn enter_no_output(&mut self) {
        self.no_output_depth += 1;
    }

    pub(crate) fn exit_no_output(&mut self) {
        self.no_output_depth = self.no_output_depth.saturating_sub(1);
    }

    pub(crate) fn suppresses_output(&self) -> bool {
        self.no_output_depth > 0
    }
}

pub(crate) struct FragmentOutputWalkState<'context, 'state> {
    pub(crate) locals: &'state mut SymbolicLocalState,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) analysis: &'state mut HelperSummary,
    pub(crate) outputs: &'state mut Vec<HelperFragmentOutputUse>,
}

#[cfg(test)]
#[path = "tests/helper_walk_state.rs"]
mod tests;
