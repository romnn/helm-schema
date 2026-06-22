use std::collections::BTreeSet;

use test_util::prelude::sim_assert_eq;

use super::{ActivatedHelperRange, HelperRangeRuntimePlan};
use crate::abstract_value::AbstractValue;
use crate::helper_range_frame::RangeFrame;
use crate::helper_walk_state::{HelperRuntimeControlState, HelperRuntimeLocals};
use crate::range_action_plan::RangeActionPlan;

#[test]
fn activate_range_plan_installs_predicates_binding_and_frame() {
    let plan = HelperRangeRuntimePlan {
        guard_paths: BTreeSet::from(["serviceAccount.create".to_string()]),
        action: RangeActionPlan::empty(),
        frame: RangeFrame::unknown(),
        non_exact_variable_binding: Some((
            "item".to_string(),
            AbstractValue::ValuesPath("serviceAccount.*".to_string()),
        )),
        range_fragment_value: Some(AbstractValue::ValuesPath("serviceAccount".to_string())),
    };
    let mut control = HelperRuntimeControlState::for_value(None);
    let mut locals = HelperRuntimeLocals::default();

    let ActivatedHelperRange {
        guard_paths,
        action,
        range_fragment_value,
    } = plan.activate(&mut control, &mut locals);

    sim_assert_eq!(
        have: control.active_output_predicates().iter().cloned().collect::<Vec<_>>().len(),
        want: 1
    );
    sim_assert_eq!(
        have: locals.bindings.get("item").cloned(),
        want: Some(AbstractValue::ValuesPath("serviceAccount.*".to_string()))
    );
    sim_assert_eq!(have: control.range_iteration_count(), want: 1);
    sim_assert_eq!(
        have: guard_paths,
        want: BTreeSet::from(["serviceAccount.create".to_string()])
    );
    sim_assert_eq!(have: action.has_header, want: false);
    sim_assert_eq!(
        have: range_fragment_value,
        want: Some(AbstractValue::ValuesPath("serviceAccount".to_string()))
    );
}
