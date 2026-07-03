use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_summary::{HelperFragmentOutputUse, HelperSummary};
use crate::symbolic_local_state::SymbolicLocalState;
use helm_schema_core::Predicate;

/// One dot (`.`) binding as each helper analysis domain sees it: value
/// analysis reads the context-value projection (`helper`), fragment-output
/// analysis reads the raw fragment shape (`fragment`). The domains interpret
/// the same binding differently on purpose (see
/// `plan/helper-single-walker-rewrite-postmortem.md`); this frame unifies only
/// where the pair is stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DotFrame {
    pub(crate) helper: Option<AbstractValue>,
    pub(crate) fragment: Option<AbstractValue>,
}

/// Which analysis domains an active predicate narrows. Condition
/// alternatives (`else` branches) narrow value analysis without annotating
/// fragment output metadata, so the fragment view is always a subset of the
/// value view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PredicateScope {
    Both,
    ValueOnly,
}

#[derive(Clone)]
pub(crate) struct HelperRangeIteration {
    pub(crate) dot: DotFrame,
    pub(crate) variable_binding: Option<(String, AbstractValue)>,
}

#[derive(Clone, Default)]
pub(crate) struct RangeFrame {
    pub(crate) definitely_nonempty: bool,
    pub(crate) iterations: Option<Vec<HelperRangeIteration>>,
}

#[derive(Clone)]
pub(crate) struct HelperRuntimeControlState {
    dot_stack: Vec<DotFrame>,
    active_predicates: BTreeMap<Predicate, PredicateScope>,
    active_source_relations: Vec<BTreeSet<String>>,
    range_frames: Vec<RangeFrame>,
    no_output_depth: usize,
}

#[derive(Clone)]
pub(crate) struct HelperRuntimeControlSnapshot {
    dot_stack_len: usize,
    active_predicates: BTreeMap<Predicate, PredicateScope>,
    active_source_relations: Vec<BTreeSet<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HelperRangeJoinBehavior {
    MergeAllOutcomes,
    PromoteBodyOutcome,
}

impl HelperRuntimeControlState {
    pub(crate) fn for_fragment(dot: DotFrame) -> Self {
        Self {
            dot_stack: vec![dot],
            active_predicates: BTreeMap::new(),
            active_source_relations: Vec::new(),
            range_frames: Vec::new(),
            no_output_depth: 0,
        }
    }

    pub(crate) fn current_dot(&self) -> DotFrame {
        self.dot_stack.last().cloned().unwrap_or_default()
    }

    pub(crate) fn current_helper_dot(&self) -> Option<&AbstractValue> {
        self.dot_stack
            .last()
            .and_then(|frame| frame.helper.as_ref())
    }

    pub(crate) fn current_fragment_dot(&self) -> Option<&AbstractValue> {
        self.dot_stack
            .last()
            .and_then(|frame| frame.fragment.as_ref())
    }

    pub(crate) fn active_output_predicates(&self) -> BTreeSet<Predicate> {
        self.active_predicates.keys().cloned().collect()
    }

    pub(crate) fn active_fragment_predicates(&self) -> BTreeSet<Predicate> {
        self.active_predicates
            .iter()
            .filter(|(_, scope)| **scope == PredicateScope::Both)
            .map(|(predicate, _)| predicate.clone())
            .collect()
    }

    pub(crate) fn active_source_relations(&self) -> &Vec<BTreeSet<String>> {
        &self.active_source_relations
    }

    pub(crate) fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() {
            self.active_predicates
                .insert(predicate, PredicateScope::Both);
        }
    }

    pub(crate) fn push_value_predicate_if_absent(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() {
            self.active_predicates
                .entry(predicate)
                .or_insert(PredicateScope::ValueOnly);
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
        self.dot_stack.push(DotFrame {
            helper: binding.as_ref().map(AbstractValue::to_context_value),
            fragment: binding,
        });
    }

    pub(crate) fn snapshot(&self) -> HelperRuntimeControlSnapshot {
        HelperRuntimeControlSnapshot {
            dot_stack_len: self.dot_stack.len(),
            active_predicates: self.active_predicates.clone(),
            active_source_relations: self.active_source_relations.clone(),
        }
    }

    pub(crate) fn restore(&mut self, snapshot: &HelperRuntimeControlSnapshot) {
        self.dot_stack.truncate(snapshot.dot_stack_len);
        self.active_predicates = snapshot.active_predicates.clone();
        self.active_source_relations = snapshot.active_source_relations.clone();
    }

    pub(crate) fn push_range_frame(&mut self, frame: RangeFrame) {
        self.range_frames.push(frame);
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
        self.dot_stack.push(iteration.dot);
    }

    pub(crate) fn exit_range_iteration(&mut self) {
        if self
            .range_frames
            .last()
            .is_some_and(|frame| frame.iterations.is_some())
        {
            self.dot_stack.pop();
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
