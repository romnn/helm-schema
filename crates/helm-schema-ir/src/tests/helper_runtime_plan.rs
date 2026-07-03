use std::collections::BTreeSet;

use test_util::prelude::sim_assert_eq;

use super::HelperRangeRuntimePlan;
use crate::abstract_value::AbstractValue;
use crate::helper_walk_state::{DotFrame, HelperRuntimeControlState, RangeFrame};
use crate::symbolic_local_state::SymbolicLocalState;

#[test]
fn activate_range_plan_installs_predicates_binding_and_frame() {
    let plan = HelperRangeRuntimePlan {
        guard_paths: BTreeSet::from(["serviceAccount.create".to_string()]),
        dot_binding: None,
        frame: RangeFrame {
            definitely_nonempty: false,
            iterations: None,
        },
        non_exact_variable_binding: Some((
            "item".to_string(),
            AbstractValue::ValuesPath("serviceAccount.*".to_string()),
        )),
        range_fragment_value: Some(AbstractValue::ValuesPath("serviceAccount".to_string())),
    };
    let mut control = HelperRuntimeControlState::for_fragment(DotFrame::default());
    let mut locals = SymbolicLocalState::default();

    plan.activate(&mut control, &mut locals);

    sim_assert_eq!(
        have: control.active_output_predicates().len(),
        want: 1
    );
    sim_assert_eq!(
        have: locals.fragment_values.get("item").cloned(),
        want: Some(AbstractValue::ValuesPath("serviceAccount.*".to_string()))
    );
    sim_assert_eq!(have: control.range_iteration_count(), want: 1);
    sim_assert_eq!(
        have: plan.guard_paths,
        want: BTreeSet::from(["serviceAccount.create".to_string()])
    );
    sim_assert_eq!(
        have: plan.range_fragment_value,
        want: Some(AbstractValue::ValuesPath("serviceAccount".to_string()))
    );
}
