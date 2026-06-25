use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_assignment::merge_fragment_locals;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta, HelperSummary};
use crate::predicate::Predicate;

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

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HelperRuntimeLocals {
    pub(crate) bindings: HashMap<String, AbstractValue>,
    pub(crate) default_paths: HashMap<String, BTreeSet<String>>,
}

impl HelperRuntimeLocals {
    pub(crate) fn merge(mut self, other: Self) -> Self {
        self.bindings = merge_fragment_locals(self.bindings, other.bindings);
        self.default_paths = merge_default_paths(self.default_paths, other.default_paths);
        self
    }

    pub(crate) fn set_default_paths(&mut self, variable: &str, paths: BTreeSet<String>) {
        if paths.is_empty() {
            self.default_paths.remove(variable);
        } else {
            self.default_paths.insert(variable.to_string(), paths);
        }
    }
}

#[derive(Clone)]
pub(crate) struct HelperRuntimeControlState {
    helper_dot_stack: Vec<Option<AbstractValue>>,
    fragment_dot_stack: Option<Vec<Option<AbstractValue>>>,
    active_output_predicates: BTreeSet<Predicate>,
    range_frames: Vec<RangeFrame>,
    no_output_depth: usize,
}

#[derive(Clone)]
pub(crate) struct HelperRuntimeControlSnapshot {
    helper_dot_stack_len: usize,
    fragment_dot_stack_len: Option<usize>,
    active_output_predicates: BTreeSet<Predicate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HelperRangeJoinBehavior {
    MergeAllOutcomes,
    PromoteBodyOutcome,
}

impl HelperRuntimeControlState {
    pub(crate) fn for_value(current_dot: Option<&AbstractValue>) -> Self {
        Self {
            helper_dot_stack: vec![current_dot.cloned()],
            fragment_dot_stack: None,
            active_output_predicates: BTreeSet::new(),
            range_frames: Vec::new(),
            no_output_depth: 0,
        }
    }

    pub(crate) fn for_fragment(
        current_dot: Option<&AbstractValue>,
        current_dot_fragment: Option<&AbstractValue>,
    ) -> Self {
        Self {
            helper_dot_stack: vec![current_dot.cloned()],
            fragment_dot_stack: Some(vec![current_dot_fragment.cloned()]),
            active_output_predicates: BTreeSet::new(),
            range_frames: Vec::new(),
            no_output_depth: 0,
        }
    }

    pub(crate) fn current_helper_dot(&self) -> Option<&AbstractValue> {
        self.helper_dot_stack.last().and_then(Option::as_ref)
    }

    pub(crate) fn current_fragment_dot(&self) -> Option<&AbstractValue> {
        self.fragment_dot_stack
            .as_ref()
            .and_then(|stack| stack.last())
            .and_then(Option::as_ref)
    }

    pub(crate) fn active_output_predicates(&self) -> &BTreeSet<Predicate> {
        &self.active_output_predicates
    }

    pub(crate) fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() {
            self.active_output_predicates.insert(predicate);
        }
    }

    pub(crate) fn extend_truthy_predicates(
        &mut self,
        guard_paths: impl IntoIterator<Item = String>,
    ) {
        self.active_output_predicates
            .extend(guard_paths.into_iter().map(Predicate::truthy_path));
    }

    pub(crate) fn push_effect_dot_binding(&mut self, binding: Option<AbstractValue>) {
        if let Some(fragment_dot_stack) = &mut self.fragment_dot_stack {
            fragment_dot_stack.push(binding.clone());
        }
        self.helper_dot_stack
            .push(binding.map(|binding| binding.to_context_value()));
    }

    pub(crate) fn snapshot(&self) -> HelperRuntimeControlSnapshot {
        HelperRuntimeControlSnapshot {
            helper_dot_stack_len: self.helper_dot_stack.len(),
            fragment_dot_stack_len: self.fragment_dot_stack.as_ref().map(Vec::len),
            active_output_predicates: self.active_output_predicates.clone(),
        }
    }

    pub(crate) fn restore(&mut self, snapshot: &HelperRuntimeControlSnapshot) {
        self.helper_dot_stack
            .truncate(snapshot.helper_dot_stack_len);
        if let Some(fragment_dot_stack) = &mut self.fragment_dot_stack
            && let Some(fragment_dot_stack_len) = snapshot.fragment_dot_stack_len
        {
            fragment_dot_stack.truncate(fragment_dot_stack_len);
        }
        self.active_output_predicates = snapshot.active_output_predicates.clone();
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

    pub(crate) fn enter_range_iteration(&mut self, index: usize, locals: &mut HelperRuntimeLocals) {
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
            locals.bindings.insert(variable, binding);
        }
        self.helper_dot_stack.push(iteration.helper_dot_binding);
        if let Some(fragment_dot_stack) = &mut self.fragment_dot_stack {
            fragment_dot_stack.push(iteration.fragment_dot_binding);
        }
    }

    pub(crate) fn exit_range_iteration(&mut self) {
        if self
            .range_frames
            .last()
            .is_some_and(|frame| frame.iterations.is_some())
        {
            self.helper_dot_stack.pop();
            if let Some(fragment_dot_stack) = &mut self.fragment_dot_stack {
                fragment_dot_stack.pop();
            }
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

pub(crate) struct HelperValuesWalkState<'context, 'state> {
    pub(crate) locals: &'state mut HelperRuntimeLocals,
    pub(crate) local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) analysis: &'state mut HelperSummary,
}

pub(crate) struct FragmentOutputWalkState<'context, 'state> {
    pub(crate) locals: &'state mut HelperRuntimeLocals,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) outputs: &'state mut Vec<HelperFragmentOutputUse>,
}

fn merge_default_paths(
    mut base: HashMap<String, BTreeSet<String>>,
    other: HashMap<String, BTreeSet<String>>,
) -> HashMap<String, BTreeSet<String>> {
    base.retain(|key, base_paths| {
        let Some(other_paths) = other.get(key) else {
            return false;
        };
        base_paths.extend(other_paths.iter().cloned());
        true
    });
    base
}

#[cfg(test)]
#[path = "tests/helper_walk_state.rs"]
mod tests;
