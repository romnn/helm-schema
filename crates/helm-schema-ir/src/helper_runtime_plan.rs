use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{TemplateExpr, TemplateHeader, range_variable_name_expr};

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::fragment_expr_eval::{FragmentLocalFacts, helper_result_from_expr_with_fragment_locals};
use crate::helper_summary::HelperOutputMeta;
use crate::helper_walk_state::{HelperRangeIteration, HelperRuntimeControlState, RangeFrame};
use crate::symbolic_local_state::SymbolicLocalState;
use crate::value_path_context::ValuePathContext;
use crate::value_path_context::computed_with_body_fragment_value_expr;
use helm_schema_core::Predicate;

#[derive(Clone)]
pub(crate) struct HelperConditionPlan {
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) predicate: Predicate,
    pub(crate) source_relations: Vec<BTreeSet<String>>,
    pub(crate) dot_binding: Option<AbstractValue>,
}

#[derive(Clone)]
pub(crate) struct HelperRangeRuntimePlan {
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) dot_binding: Option<AbstractValue>,
    pub(crate) frame: RangeFrame,
    pub(crate) non_exact_variable_binding: Option<(String, AbstractValue)>,
    pub(crate) range_fragment_value: Option<AbstractValue>,
}

impl HelperRangeRuntimePlan {
    pub(crate) fn activate(
        &self,
        control: &mut HelperRuntimeControlState,
        locals: &mut SymbolicLocalState,
    ) {
        control.extend_truthy_predicates(self.guard_paths.iter().cloned());
        if let Some((variable, binding)) = &self.non_exact_variable_binding {
            locals
                .fragment_values
                .insert(variable.clone(), binding.clone());
        }
        if self.frame.iterations.is_none() {
            control.push_effect_dot_binding(self.dot_binding.clone());
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
        guard_paths: facts.guard_paths,
        predicate: facts.predicate,
        source_relations: facts.source_relations,
        dot_binding: None,
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
        guard_paths: facts.guard_paths,
        predicate: facts.predicate,
        source_relations: facts.source_relations,
        dot_binding: body_dot,
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
) -> HelperRangeRuntimePlan {
    let Some(header) = header else {
        return HelperRangeRuntimePlan {
            guard_paths: BTreeSet::new(),
            dot_binding: None,
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
    let range_fragment_value = range_iterable_binding_expr(
        header.expr(),
        local_bindings,
        current_dot_fragment,
        context,
        seen,
    );
    let range_variable = range_variable_name_expr(header.expr());
    let exact_iterations = if let Some(AbstractValue::List(items)) = &range_fragment_value {
        Some(
            items
                .iter()
                .map(|item| HelperRangeIteration {
                    helper_dot_binding: Some(item.to_context_value()),
                    fragment_dot_binding: Some(item.clone()),
                    variable_binding: range_variable
                        .as_ref()
                        .map(|variable| (variable.clone(), item.clone())),
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let non_exact_variable_binding = if exact_iterations.is_none() {
        range_variable.zip(
            range_fragment_value
                .as_ref()
                .and_then(AbstractValue::fragment_range_item)
                .map(|binding| binding.to_context_value()),
        )
    } else {
        None
    };
    let dot_binding = range_fragment_value
        .as_ref()
        .and_then(AbstractValue::fragment_range_item)
        .map(|binding| binding.to_context_value());
    let frame = RangeFrame {
        definitely_nonempty: range_fragment_value
            .as_ref()
            .is_some_and(AbstractValue::definitely_nonempty_iterable),
        iterations: exact_iterations,
    };

    HelperRangeRuntimePlan {
        dot_binding,
        frame,
        guard_paths,
        non_exact_variable_binding,
        range_fragment_value,
    }
}

struct BranchConditionFacts {
    guard_paths: BTreeSet<String>,
    predicate: Predicate,
    source_relations: Vec<BTreeSet<String>>,
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
        source_relations: condition_source_relations(expr, local_bindings, local_output_meta),
    }
}

fn condition_source_relations(
    expr: &TemplateExpr,
    local_bindings: &HashMap<String, AbstractValue>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
) -> Vec<BTreeSet<String>> {
    let Some(variable) = condition_local_variable(expr) else {
        return Vec::new();
    };
    let mut sources = local_bindings
        .get(variable)
        .map(AbstractValue::fragment_source_paths)
        .unwrap_or_default();
    if let Some(meta_by_path) = local_output_meta.get(variable) {
        sources.extend(meta_by_path.keys().cloned());
    }
    if sources.len() > 1 {
        vec![sources]
    } else {
        Vec::new()
    }
}

fn condition_local_variable(expr: &TemplateExpr) -> Option<&str> {
    match expr.deparen() {
        TemplateExpr::Variable(name) if !name.is_empty() => Some(name.as_str()),
        TemplateExpr::Pipeline(stages) => stages.first().and_then(condition_local_variable),
        _ => None,
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

fn range_iterable_binding_expr(
    expr: &TemplateExpr,
    local_bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<AbstractValue> {
    let value = match expr.deparen() {
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            value.as_ref()
        }
        expr => expr,
    };
    context.fragment_value_from_expr(value, local_bindings, current_dot, seen)
}

#[cfg(test)]
#[path = "tests/helper_runtime_plan.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/helper_runtime_guards.rs"]
mod guard_tests;
