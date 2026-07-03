use std::collections::{BTreeSet, HashMap};

use test_util::prelude::sim_assert_eq;

use super::{DotFrame, HelperRangeJoinBehavior, HelperRuntimeControlState, RangeFrame};
use crate::abstract_value::AbstractValue;
use crate::symbolic_local_state::SymbolicLocalState;

#[test]
fn merge_intersects_default_paths_by_branch_presence() {
    let mut base = SymbolicLocalState::default();
    base.default_paths = HashMap::from([
        (
            "serviceAccount".to_string(),
            BTreeSet::from(["left.default".to_string()]),
        ),
        (
            "leftOnly".to_string(),
            BTreeSet::from(["left.only".to_string()]),
        ),
    ]);
    let mut other = SymbolicLocalState::default();
    other.default_paths = HashMap::from([
        (
            "serviceAccount".to_string(),
            BTreeSet::from(["right.default".to_string()]),
        ),
        (
            "rightOnly".to_string(),
            BTreeSet::from(["right.only".to_string()]),
        ),
    ]);

    let merged = base.merge_helper_outcome(other);

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
    let mut base = SymbolicLocalState::default();
    base.fragment_values = HashMap::from([(
        "config".to_string(),
        AbstractValue::ValuesPath("left".to_string()),
    )]);
    let mut other = SymbolicLocalState::default();
    other.fragment_values = HashMap::from([(
        "config".to_string(),
        AbstractValue::ValuesPath("right".to_string()),
    )]);

    let merged = base.merge_helper_outcome(other);

    sim_assert_eq!(
        have: merged.fragment_values.get("config").cloned(),
        want: Some(AbstractValue::Choice(BTreeSet::from([
            AbstractValue::ValuesPath("left".to_string()),
            AbstractValue::ValuesPath("right".to_string())
        ])))
    );
}

#[test]
fn fragment_control_state_tracks_fragment_and_helper_dot() {
    let mut state = HelperRuntimeControlState::for_fragment(DotFrame {
        helper: Some(AbstractValue::ValuesPath("root".to_string())),
        fragment: Some(AbstractValue::ValuesPath("fragment.root".to_string())),
    });

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
    let mut state = HelperRuntimeControlState::for_fragment(DotFrame::default());
    state.push_range_frame(RangeFrame {
        definitely_nonempty: true,
        iterations: None,
    });
    let snapshot = state.snapshot();

    let behavior = state.prepare_range_join(&snapshot);

    sim_assert_eq!(have: behavior, want: HelperRangeJoinBehavior::PromoteBodyOutcome);
}

#[test]
fn prepare_range_join_merges_when_frame_is_not_definitely_nonempty() {
    let mut state = HelperRuntimeControlState::for_fragment(DotFrame::default());
    state.push_range_frame(RangeFrame {
        definitely_nonempty: false,
        iterations: None,
    });
    let snapshot = state.snapshot();

    let behavior = state.prepare_range_join(&snapshot);

    sim_assert_eq!(have: behavior, want: HelperRangeJoinBehavior::MergeAllOutcomes);
}
