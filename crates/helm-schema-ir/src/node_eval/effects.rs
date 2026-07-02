use crate::abstract_value::AbstractValue;
use crate::condition_action_plan::ConditionActionPlan;
use crate::range_action_plan::RangeActionPlan;
use crate::{Guard, ValueKind, YamlPath};
use helm_schema_core::Predicate;

pub(crate) trait NodeActionEffectSink {
    fn push_predicate_if_absent(&mut self, predicate: Predicate);

    fn push_dot_binding(&mut self, _binding: Option<AbstractValue>) {}

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

pub(crate) fn activate_if_condition_plan(
    sink: &mut impl NodeActionEffectSink,
    plan: &ConditionActionPlan,
) {
    let guards = plan.predicate.contract_guards();
    for value in &plan.bound_values {
        sink.observe_value_use(value.clone(), YamlPath(Vec::new()), ValueKind::Scalar);
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
        sink.push_predicate_if_absent(plan.predicate.clone());
    }
}

/// Push each contract guard of `predicate`; when the predicate decodes to no
/// contract guards, push the raw predicate so the branch fact is not lost.
/// Returns the decoded guards for callers that also observe guard-path uses.
pub(crate) fn push_predicate_contract_guards(
    sink: &mut impl NodeActionEffectSink,
    predicate: &Predicate,
) -> Vec<Guard> {
    let guards = predicate.contract_guards();
    for guard in &guards {
        sink.push_predicate_if_absent(Predicate::from(guard.clone()));
    }
    if guards.is_empty() {
        sink.push_predicate_if_absent(predicate.clone());
    }
    guards
}

pub(crate) fn activate_with_condition_plan(
    sink: &mut impl NodeActionEffectSink,
    plan: &ConditionActionPlan,
) {
    // Push the With predicate before emitting header scalar uses so the
    // emitted contract guards on those uses include `Guard::With`.
    // The schema generator uses that marker to identify with-header reads.
    let guards = push_predicate_contract_guards(sink, &plan.predicate);

    for value in &plan.bound_values {
        sink.observe_value_use(value.clone(), YamlPath(Vec::new()), ValueKind::Scalar);
    }

    for guard in &guards {
        for path in guard.value_paths() {
            sink.observe_value_use(path.to_string(), YamlPath(Vec::new()), ValueKind::Scalar);
        }
    }
    sink.push_dot_binding(plan.dot_binding.clone());
}

pub(crate) fn activate_range_action_plan(
    sink: &mut impl NodeActionEffectSink,
    plan: &RangeActionPlan,
    current_path: &YamlPath,
) {
    if let Some((variable, literals)) = &plan.literal_range {
        sink.insert_range_domain(variable.clone(), literals.clone());
    }
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

    sink.push_dot_binding(plan.dot_binding.clone());
}
