use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{TemplateExpr, TemplateHeader};

use crate::abstract_value::AbstractValue;
use crate::condition_action_plan::ConditionActionPlan;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::fragment_expr_eval::{FragmentLocalFacts, helper_result_from_expr_with_fragment_locals};
use crate::helper_range_frame::RangeFrame;
use crate::helper_range_plan::{
    HelperRangeIteration, NonExactRangeVariableBinding, plan_helper_range_binding,
};
use crate::helper_summary::HelperOutputMeta;
use crate::helper_walk_state::{HelperRuntimeControlState, HelperRuntimeLocals};
use crate::predicate::Predicate;
use crate::range_action_plan::RangeActionPlan;
use crate::value_path_context::ValuePathContext;
use crate::value_path_context::computed_with_body_fragment_value_expr;

#[derive(Clone)]
pub(crate) struct HelperConditionPlan {
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) action: ConditionActionPlan,
}

#[derive(Clone)]
pub(crate) struct HelperRangeRuntimePlan {
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) action: RangeActionPlan,
    pub(crate) frame: RangeFrame<HelperRangeIteration>,
    pub(crate) non_exact_variable_binding: Option<(String, AbstractValue)>,
    pub(crate) range_fragment_value: Option<AbstractValue>,
}

impl HelperRangeRuntimePlan {
    pub(crate) fn activate(
        &self,
        control: &mut HelperRuntimeControlState,
        locals: &mut HelperRuntimeLocals,
    ) {
        control.extend_truthy_predicates(self.guard_paths.iter().cloned());
        if let Some((variable, binding)) = &self.non_exact_variable_binding {
            locals.bindings.insert(variable.clone(), binding.clone());
        }
        control.push_range_frame(self.frame.clone());
    }
}

pub(crate) fn helper_if_condition_plan(
    header: &TemplateHeader,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    local_default_paths: &HashMap<String, BTreeSet<String>>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    apply_alternative_predicate: bool,
) -> HelperConditionPlan {
    let facts = branch_condition_facts_for_expr(
        header.expr(),
        bindings,
        current_dot,
        local_bindings,
        local_default_paths,
        local_output_meta,
        context,
        seen,
    );
    HelperConditionPlan {
        action: ConditionActionPlan {
            predicate: facts.predicate,
            bound_values: Vec::new(),
            dot_binding: None,
            apply_alternative_predicate,
        },
        guard_paths: facts.guard_paths,
    }
}

pub(crate) fn helper_with_condition_plan(
    header: &TemplateHeader,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    local_default_paths: &HashMap<String, BTreeSet<String>>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    apply_alternative_predicate: bool,
) -> HelperConditionPlan {
    let facts = branch_condition_facts_for_expr(
        header.expr(),
        bindings,
        current_dot,
        local_bindings,
        local_default_paths,
        local_output_meta,
        context,
        seen,
    );
    let body_dot = computed_with_body_fragment_value_expr(
        header.expr(),
        bindings,
        local_bindings,
        context,
        current_dot_fragment,
        current_dot,
    );
    HelperConditionPlan {
        action: ConditionActionPlan {
            predicate: facts.predicate,
            bound_values: Vec::new(),
            dot_binding: body_dot,
            apply_alternative_predicate,
        },
        guard_paths: facts.guard_paths,
    }
}

pub(crate) fn helper_range_runtime_plan(
    header: Option<&TemplateHeader>,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    non_exact_range_variable_binding: NonExactRangeVariableBinding,
    use_fragment_dot: bool,
) -> HelperRangeRuntimePlan {
    let Some(header) = header else {
        return HelperRangeRuntimePlan {
            guard_paths: BTreeSet::new(),
            action: RangeActionPlan::empty(),
            frame: RangeFrame {
                definitely_nonempty: false,
                iterations: None,
            },
            non_exact_variable_binding: None,
            range_fragment_value: None,
        };
    };

    let guard_paths = branch_guard_paths_for_expr(
        header.expr(),
        bindings,
        current_dot,
        local_bindings,
        context,
        seen,
    );
    let mut range_plan = plan_helper_range_binding(
        header,
        local_bindings,
        current_dot_fragment,
        context,
        seen,
        non_exact_range_variable_binding,
    );
    let non_exact_variable_binding = range_plan.take_non_exact_variable_binding();
    let range_fragment_value = range_plan.range_fragment_value().cloned();
    let (frame, dot_binding) = if use_fragment_dot {
        (
            range_plan.fragment_output_frame(),
            range_plan.fragment_output_body_dot(),
        )
    } else {
        (
            range_plan.helper_value_frame(),
            range_plan.helper_value_body_dot(),
        )
    };

    HelperRangeRuntimePlan {
        action: RangeActionPlan::dot_binding(dot_binding, range_plan.apply_dot_binding()),
        frame,
        guard_paths,
        non_exact_variable_binding,
        range_fragment_value,
    }
}

struct BranchConditionFacts {
    guard_paths: BTreeSet<String>,
    predicate: Predicate,
}

fn branch_condition_facts_for_expr(
    expr: &TemplateExpr,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    local_default_paths: &HashMap<String, BTreeSet<String>>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BranchConditionFacts {
    let guard_paths =
        branch_guard_paths_for_expr(expr, bindings, current_dot, local_bindings, context, seen);
    let range_domains = HashMap::new();
    let get_bindings = HashMap::new();
    let value_path_context = ValuePathContext {
        root_bindings: bindings,
        template_bindings: local_bindings,
        range_domains: &range_domains,
        get_bindings: &get_bindings,
        template_default_paths: local_default_paths,
        template_output_meta: local_output_meta,
        fragment_context: context,
        current_dot_fragment: None,
        current_dot_binding: current_dot.cloned(),
    };
    let predicate = value_path_context.condition_predicate_expr(expr);
    let predicate = if predicate.is_trivial() {
        Predicate::all(
            guard_paths
                .iter()
                .cloned()
                .map(Predicate::truthy_path)
                .collect(),
        )
    } else {
        predicate
    };
    BranchConditionFacts {
        guard_paths,
        predicate,
    }
}

fn branch_guard_paths_for_expr(
    expr: &TemplateExpr,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BTreeSet<String> {
    let env = EvalEnv::from_helper_context(Some(bindings), current_dot).without_helper_call_args();
    let mut branch_guard_paths = eval_expr(expr, &env).effects.reads;
    let local_env = EvalEnv {
        locals: local_bindings.clone(),
        skip_helper_call_args: true,
        ..EvalEnv::default()
    };
    branch_guard_paths.extend(eval_expr(expr, &local_env).effects.local_source_paths());

    branch_guard_paths.extend(
        helper_result_from_expr_with_fragment_locals(
            expr,
            FragmentLocalFacts::bindings_only(local_bindings),
            Some(bindings),
            current_dot,
            context,
            seen,
        )
        .effects
        .helper_summary
        .dependency_relevant_paths(),
    );
    branch_guard_paths
}

#[cfg(test)]
#[path = "tests/helper_runtime_plan.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/helper_runtime_guards.rs"]
mod guard_tests;
