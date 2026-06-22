use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_assignment::merge_fragment_locals;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_range_frame::RangeFrame;
use crate::helper_range_plan::HelperRangeIteration;
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta, HelperSummary};
use crate::predicate::Predicate;

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
}

#[derive(Clone)]
pub(crate) struct HelperRuntimeControlState {
    helper_dot_stack: Vec<Option<AbstractValue>>,
    fragment_dot_stack: Option<Vec<Option<AbstractValue>>>,
    active_output_predicates: BTreeSet<Predicate>,
    range_frames: Vec<RangeFrame<HelperRangeIteration>>,
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

pub(crate) enum HelperRuntimeScopeJoin<T> {
    Merge(Vec<T>),
    Promote(T),
    Noop,
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

    pub(crate) fn push_range_frame(&mut self, frame: RangeFrame<HelperRangeIteration>) {
        self.range_frames.push(frame);
    }

    pub(crate) fn prepare_branch_join(&mut self, snapshot: &HelperRuntimeControlSnapshot) {
        self.restore(snapshot);
    }

    pub(crate) fn branch_join_outcomes<T>(
        &mut self,
        snapshot: &HelperRuntimeControlSnapshot,
        outcomes: Vec<T>,
    ) -> Vec<T> {
        self.prepare_branch_join(snapshot);
        outcomes
    }

    pub(crate) fn prepare_range_join(
        &mut self,
        snapshot: &HelperRuntimeControlSnapshot,
    ) -> HelperRangeJoinBehavior {
        self.restore(snapshot);
        if self
            .range_frames
            .pop()
            .is_some_and(|frame| frame.is_definitely_nonempty())
        {
            HelperRangeJoinBehavior::PromoteBodyOutcome
        } else {
            HelperRangeJoinBehavior::MergeAllOutcomes
        }
    }

    pub(crate) fn range_join_outcomes<T>(
        &mut self,
        snapshot: &HelperRuntimeControlSnapshot,
        outcomes: Vec<T>,
    ) -> HelperRuntimeScopeJoin<T> {
        match self.prepare_range_join(snapshot) {
            HelperRangeJoinBehavior::PromoteBodyOutcome => outcomes
                .into_iter()
                .next()
                .map(HelperRuntimeScopeJoin::Promote)
                .unwrap_or(HelperRuntimeScopeJoin::Noop),
            HelperRangeJoinBehavior::MergeAllOutcomes => HelperRuntimeScopeJoin::Merge(outcomes),
        }
    }

    pub(crate) fn range_iteration_count(&self) -> usize {
        self.range_frames
            .last()
            .map(RangeFrame::iteration_count)
            .unwrap_or(1)
    }

    pub(crate) fn enter_range_iteration(&mut self, index: usize, locals: &mut HelperRuntimeLocals) {
        let Some(iteration) = self
            .range_frames
            .last()
            .and_then(|frame| frame.iteration(index))
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
            .is_some_and(RangeFrame::has_exact_iterations)
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
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use test_util::prelude::sim_assert_eq;

    use super::{HelperRangeJoinBehavior, HelperRuntimeControlState, HelperRuntimeLocals};
    use crate::abstract_value::AbstractValue;
    use crate::helper_range_frame::RangeFrame;

    #[test]
    fn merge_intersects_default_paths_by_branch_presence() {
        let base = HelperRuntimeLocals {
            bindings: HashMap::new(),
            default_paths: HashMap::from([
                (
                    "serviceAccount".to_string(),
                    BTreeSet::from(["left.default".to_string()]),
                ),
                (
                    "leftOnly".to_string(),
                    BTreeSet::from(["left.only".to_string()]),
                ),
            ]),
        };
        let other = HelperRuntimeLocals {
            bindings: HashMap::new(),
            default_paths: HashMap::from([
                (
                    "serviceAccount".to_string(),
                    BTreeSet::from(["right.default".to_string()]),
                ),
                (
                    "rightOnly".to_string(),
                    BTreeSet::from(["right.only".to_string()]),
                ),
            ]),
        };

        let merged = base.merge(other);

        sim_assert_eq!(
            have: merged.default_paths,
            want: HashMap::from([(
                "serviceAccount".to_string(),
                BTreeSet::from(["left.default".to_string(), "right.default".to_string()])
            )])
        );
    }

    #[test]
    fn merge_unions_fragment_local_bindings() {
        let base = HelperRuntimeLocals {
            bindings: HashMap::from([(
                "config".to_string(),
                AbstractValue::ValuesPath("left".to_string()),
            )]),
            default_paths: HashMap::new(),
        };
        let other = HelperRuntimeLocals {
            bindings: HashMap::from([(
                "config".to_string(),
                AbstractValue::ValuesPath("right".to_string()),
            )]),
            default_paths: HashMap::new(),
        };

        let merged = base.merge(other);

        sim_assert_eq!(
            have: merged.bindings.get("config").cloned(),
            want: Some(AbstractValue::Choice(BTreeSet::from([
                AbstractValue::ValuesPath("left".to_string()),
                AbstractValue::ValuesPath("right".to_string())
            ])))
        );
    }

    #[test]
    fn value_control_state_pushes_helper_context_dot_only() {
        let mut state = HelperRuntimeControlState::for_value(Some(&AbstractValue::ValuesPath(
            "root".to_string(),
        )));

        state.push_effect_dot_binding(Some(AbstractValue::ValuesPath("child".to_string())));

        sim_assert_eq!(
            have: state.current_helper_dot().cloned(),
            want: Some(AbstractValue::ValuesPath("child".to_string()))
        );
        assert!(state.current_fragment_dot().is_none());
    }

    #[test]
    fn fragment_control_state_tracks_fragment_and_helper_dot() {
        let mut state = HelperRuntimeControlState::for_fragment(
            Some(&AbstractValue::ValuesPath("root".to_string())),
            Some(&AbstractValue::ValuesPath("fragment.root".to_string())),
        );

        state.push_effect_dot_binding(Some(AbstractValue::ValuesPath("child".to_string())));

        sim_assert_eq!(
            have: state.current_helper_dot().cloned(),
            want: Some(AbstractValue::ValuesPath("child".to_string()))
        );
        sim_assert_eq!(
            have: state.current_fragment_dot().cloned(),
            want: Some(AbstractValue::ValuesPath("child".to_string()))
        );
    }

    #[test]
    fn prepare_range_join_promotes_body_for_definitely_nonempty_frame() {
        let mut state = HelperRuntimeControlState::for_value(None);
        state.push_range_frame(RangeFrame::new(true, None));
        let snapshot = state.snapshot();

        let behavior = state.prepare_range_join(&snapshot);

        sim_assert_eq!(have: behavior, want: HelperRangeJoinBehavior::PromoteBodyOutcome);
    }

    #[test]
    fn prepare_range_join_merges_when_frame_is_not_definitely_nonempty() {
        let mut state = HelperRuntimeControlState::for_value(None);
        state.push_range_frame(RangeFrame::new(false, None));
        let snapshot = state.snapshot();

        let behavior = state.prepare_range_join(&snapshot);

        sim_assert_eq!(have: behavior, want: HelperRangeJoinBehavior::MergeAllOutcomes);
    }
}
