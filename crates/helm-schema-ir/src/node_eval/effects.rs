use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::assignment_action_plan::{AssignmentActionPlan, LocalAssignmentPlan};
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::ConditionActionPlan;
use crate::fragment_assignment::AssignmentKind;
use crate::predicate::Predicate;
use crate::range_action_plan::RangeActionPlan;
use crate::{Guard, ValueKind, YamlPath};

pub(crate) trait NodeActionEffectSink {
    fn apply_get_binding(&mut self, _plan: GetBindingPlan) {}

    fn declare_fragment_value(&mut self, _variable: String, _binding: Option<AbstractValue>) {}

    fn assign_fragment_value(&mut self, _variable: String, _binding: Option<AbstractValue>) {}

    fn refresh_default_paths(&mut self, _variable: &str, _rhs_expr: &TemplateExpr) {}

    fn refresh_helper_output_meta(&mut self, _variable: String, _rhs_expr: &TemplateExpr) {}

    fn push_predicate_if_absent(&mut self, predicate: Predicate);

    fn push_dot_binding(&mut self, binding: Option<AbstractValue>);

    fn insert_range_domain(&mut self, _variable: String, _literals: Vec<String>) {}

    fn observe_value_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind) {
        self.observe_value_use_with_extra_guards(source_expr, path, kind, &[]);
    }

    fn observe_value_use_with_extra_guards(
        &mut self,
        _source_expr: String,
        _path: YamlPath,
        _kind: ValueKind,
        _extra_guards: &[Guard],
    ) {
    }
}

pub(super) fn apply_assignment_action_plan(
    sink: &mut impl NodeActionEffectSink,
    plan: AssignmentActionPlan,
) {
    if let Some(local_assignment) = plan.local_assignment {
        apply_local_assignment_plan(sink, local_assignment);
    }

    if let Some(get_binding) = plan.get_binding {
        sink.apply_get_binding(get_binding);
    }
}

fn apply_local_assignment_plan(sink: &mut impl NodeActionEffectSink, plan: LocalAssignmentPlan) {
    match plan.kind {
        AssignmentKind::Declaration => {
            sink.declare_fragment_value(plan.variable.clone(), plan.fragment_value);
        }
        AssignmentKind::Assignment => {
            sink.assign_fragment_value(plan.variable.clone(), plan.fragment_value);
        }
    }
    sink.refresh_default_paths(&plan.variable, &plan.rhs_expr);
    sink.refresh_helper_output_meta(plan.variable, &plan.rhs_expr);
}

pub(super) fn apply_if_condition_plan(
    sink: &mut impl NodeActionEffectSink,
    plan: ConditionActionPlan,
) {
    let guards = plan.contract_guards();
    for value in plan.bound_values {
        sink.observe_value_use(value, YamlPath(Vec::new()), ValueKind::Scalar);
    }

    for guard in &guards {
        for path in guard.value_paths() {
            sink.observe_value_use_with_extra_guards(
                path.to_string(),
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                std::slice::from_ref(guard),
            );
        }
        sink.push_predicate_if_absent(Predicate::from(guard.clone()));
    }
    if guards.is_empty() {
        sink.push_predicate_if_absent(plan.predicate);
    }
}

pub(super) fn apply_with_condition_plan(
    sink: &mut impl NodeActionEffectSink,
    plan: ConditionActionPlan,
) {
    let guards = plan.contract_guards();
    // Push the With predicate before emitting header scalar uses so the
    // emitted contract guards on those uses include `Guard::With`.
    // The schema generator uses that marker to identify with-header reads.
    for guard in &guards {
        sink.push_predicate_if_absent(Predicate::from(guard.clone()));
    }
    if guards.is_empty() {
        sink.push_predicate_if_absent(plan.predicate.clone());
    }

    for value in plan.bound_values {
        sink.observe_value_use(value, YamlPath(Vec::new()), ValueKind::Scalar);
    }

    for guard in &guards {
        for path in guard.value_paths() {
            sink.observe_value_use(path.to_string(), YamlPath(Vec::new()), ValueKind::Scalar);
        }
    }
    sink.push_dot_binding(plan.dot_binding);
}

pub(super) fn apply_condition_alternative_guards(
    sink: &mut impl NodeActionEffectSink,
    plan: &ConditionActionPlan,
) {
    if !plan.apply_alternative_predicate {
        return;
    }
    sink.push_predicate_if_absent(plan.predicate.negated());
}

pub(super) fn apply_range_action_plan(
    sink: &mut impl NodeActionEffectSink,
    plan: &RangeActionPlan,
    current_path: &YamlPath,
) {
    if let Some((variable, literals)) = &plan.literal_range {
        sink.insert_range_domain(variable.clone(), literals.clone());
    }
    if plan.has_header {
        for source_path in &plan.source_paths {
            let guard = Guard::Range {
                path: source_path.clone(),
            };
            if plan.emit_header_use {
                sink.observe_value_use_with_extra_guards(
                    source_path.clone(),
                    plan.guard_path.clone(),
                    ValueKind::Scalar,
                    std::slice::from_ref(&guard),
                );
            }
            sink.push_predicate_if_absent(Predicate::from(guard));
        }

        if plan.renders_mapping_entries {
            for source_path in &plan.source_paths {
                sink.observe_value_use(
                    source_path.clone(),
                    current_path.clone(),
                    ValueKind::Fragment,
                );
            }
        }
    }

    if plan.apply_dot_binding {
        sink.push_dot_binding(plan.dot_binding.clone());
    }
}
