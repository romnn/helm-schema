use std::collections::BTreeMap;
use std::rc::Rc;

use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use crate::value::{AbortReason, Coverage, DecisionEdge, Flow, State, Value};

#[test]
fn constant_value_plan_outcomes_do_not_force_an_active_guard() {
    for succeeds in [true, false] {
        let mut state = State::new(Value::nil());
        state.active = Rc::new(
            (0..64)
                .map(|index| {
                    let predicate = Predicate::truthy_path(format!("active{index}"));
                    DecisionEdge {
                        test: state.heap.arena.decision(
                            predicate.clone(),
                            predicate.negated(),
                            Coverage::Exact,
                        ),
                        taken: true,
                    }
                })
                .collect(),
        );
        let normal = super::constant_value_test(&state, succeeds);
        sim_assert_eq!(have: normal.constant_outcome(), want: Some(succeeds));
        let failures = if succeeds {
            BTreeMap::new()
        } else {
            BTreeMap::from([(AbortReason::Fail, super::constant_value_test(&state, true))])
        };
        let arena = Rc::clone(&state.heap.arena);
        let before = arena.created_runtime_nodes();
        let exits = super::finish_value_plan(
            super::ValuePlan {
                value: Value::nil(),
                normal,
                failures,
            },
            state,
        );
        sim_assert_eq!(have: exits.len(), want: 1);
        sim_assert_eq!(have: matches!(exits.first().map(|exit| &exit.flow), Some(Flow::Normal(_))), want: succeeds);
        if !succeeds {
            assert!(matches!(
                exits.first().map(|exit| &exit.flow),
                Some(Flow::Abort(AbortReason::Fail))
            ));
        }
        sim_assert_eq!(have: arena.created_runtime_nodes(), want: before);
    }
}
