use crate::assignment_action_plan::{AssignmentActionPlan, LocalAssignmentPlan};
use crate::binding::FragmentBinding;
use crate::bound_value_analysis::GetBinding;
use crate::condition_action_plan::ConditionActionPlan;
use crate::fragment_scope_eval::AssignmentKind;
use crate::output_value_emitter::ValueUseSink;
use crate::range_action_plan::RangeActionPlan;
use crate::{Guard, ValueKind, YamlPath};

pub(crate) trait NodeActionEffectSink: ValueUseSink {
    fn insert_get_binding(&mut self, variable: String, binding: GetBinding);

    fn declare_fragment_binding(&mut self, variable: String, binding: FragmentBinding);

    fn assign_fragment_binding(&mut self, variable: String, binding: FragmentBinding);

    fn refresh_default_paths(&mut self, variable: &str, rhs: &str);

    fn refresh_helper_output_meta(&mut self, variable: String, rhs: &str);

    fn push_guard_if_absent(&mut self, guard: Guard);

    fn push_dot_binding(&mut self, binding: Option<FragmentBinding>);

    fn insert_range_domain(&mut self, variable: String, literals: Vec<String>);
}

pub(crate) fn apply_assignment_action_plan(
    sink: &mut dyn NodeActionEffectSink,
    plan: AssignmentActionPlan,
) {
    if let Some((variable, binding)) = plan.get_binding {
        sink.insert_get_binding(variable, binding);
    }

    if let Some(local_assignment) = plan.local_assignment {
        apply_local_assignment_plan(sink, local_assignment);
    }
}

fn apply_local_assignment_plan(sink: &mut dyn NodeActionEffectSink, plan: LocalAssignmentPlan) {
    if let Some(binding) = plan.fragment_binding {
        match plan.kind {
            AssignmentKind::Declaration => {
                sink.declare_fragment_binding(plan.variable.clone(), binding);
            }
            AssignmentKind::Assignment => {
                sink.assign_fragment_binding(plan.variable.clone(), binding);
            }
        }
    }
    sink.refresh_default_paths(&plan.variable, &plan.rhs);
    sink.refresh_helper_output_meta(plan.variable, &plan.rhs);
}

pub(crate) fn apply_if_condition_plan(
    sink: &mut dyn NodeActionEffectSink,
    plan: ConditionActionPlan,
) {
    let guards = plan.compatibility_guards();
    for value in plan.bound_values {
        sink.emit_use(value, YamlPath(Vec::new()), ValueKind::Scalar);
    }

    for guard in &guards {
        for path in guard.value_paths() {
            sink.emit_use_with_extra_guards(
                path.to_string(),
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                std::slice::from_ref(guard),
            );
        }
        sink.push_guard_if_absent(guard.clone());
    }
}

pub(crate) fn apply_with_condition_plan(
    sink: &mut dyn NodeActionEffectSink,
    plan: ConditionActionPlan,
) {
    let guards = plan.compatibility_guards();
    // Push the With guards before emitting header scalar uses so the emitted
    // uses themselves carry the With guard. This lets the schema generator
    // identify with-header uses by the presence of a matching
    // `Guard::With { path: source_expr }` in the use's guard list.
    for guard in &guards {
        sink.push_guard_if_absent(guard.clone());
    }

    for value in plan.bound_values {
        sink.emit_use(value, YamlPath(Vec::new()), ValueKind::Scalar);
    }

    for guard in &guards {
        for path in guard.value_paths() {
            sink.emit_use(path.to_string(), YamlPath(Vec::new()), ValueKind::Scalar);
        }
    }
    sink.push_dot_binding(plan.dot_binding);
}

pub(crate) fn apply_condition_alternative_guards(
    sink: &mut dyn NodeActionEffectSink,
    plan: &ConditionActionPlan,
) {
    for guard in plan.negated_compatibility_guards() {
        sink.push_guard_if_absent(guard);
    }
}

pub(crate) fn apply_range_action_plan(
    sink: &mut dyn NodeActionEffectSink,
    plan: &RangeActionPlan,
    current_path: &YamlPath,
) {
    if let Some((variable, literals)) = &plan.literal_range {
        sink.insert_range_domain(variable.clone(), literals.clone());
    }
    if plan.header_text.is_some() {
        for source_path in &plan.source_paths {
            let guard = Guard::Range {
                path: source_path.clone(),
            };
            if plan.emit_header_use {
                sink.emit_use_with_extra_guards(
                    source_path.clone(),
                    plan.guard_path.clone(),
                    ValueKind::Scalar,
                    std::slice::from_ref(&guard),
                );
            }
            sink.push_guard_if_absent(guard);
        }

        if plan.renders_mapping_entries {
            for source_path in &plan.source_paths {
                sink.emit_use(
                    source_path.clone(),
                    current_path.clone(),
                    ValueKind::Fragment,
                );
            }
        }
    }

    sink.push_dot_binding(plan.dot_binding.clone());
}
