use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::fragment_expr_eval::{
    FragmentEvalContext, FragmentLocalFacts, helper_result_from_expr_with_fragment_locals,
};
use crate::helper_summary::HelperOutputMeta;
use crate::predicate::Predicate;
use crate::value_path_context::ValuePathContext;

pub(crate) struct BranchConditionFacts {
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) predicate: Predicate,
}

pub(crate) fn branch_condition_facts_for_expr(
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
    let value_path_context = ValuePathContext {
        root_bindings: bindings,
        template_bindings: local_bindings,
        template_default_paths: local_default_paths,
        template_output_meta: local_output_meta,
        fragment_context: context,
        current_dot_fragment: None,
        current_dot_binding: current_dot.cloned(),
    };
    let predicate = value_path_context.condition_predicate_expr(expr);
    let predicate = if predicate.is_trivial() {
        truthy_predicate_for_paths(&guard_paths)
    } else {
        predicate
    };
    BranchConditionFacts {
        guard_paths,
        predicate,
    }
}

pub(crate) fn branch_guard_paths_for_expr(
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
    branch_guard_paths.extend(eval_expr(expr, &local_env).effects.local_source_paths);

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

pub(crate) fn truthy_predicate_for_paths(paths: &BTreeSet<String>) -> Predicate {
    Predicate::all(paths.iter().cloned().map(Predicate::truthy_path).collect())
}

#[cfg(test)]
#[path = "tests/helper_runtime_guards.rs"]
mod tests;
