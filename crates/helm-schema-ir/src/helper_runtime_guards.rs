use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::predicate::Predicate;

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

    let nested = context
        .helper_summaries()
        .summarize_bound_helper_calls_in_exprs(
            std::slice::from_ref(expr),
            Some(bindings),
            current_dot,
            local_bindings,
            context,
            seen,
        );
    branch_guard_paths.extend(nested.dependency_relevant_paths());
    branch_guard_paths
}

pub(crate) fn truthy_predicate_for_paths(paths: &BTreeSet<String>) -> Predicate {
    Predicate::all(paths.iter().cloned().map(Predicate::truthy_path).collect())
}

#[cfg(test)]
#[path = "tests/helper_runtime_guards.rs"]
mod tests;
