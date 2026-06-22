use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::bound_helper_env::BoundHelperEnv;
use crate::eval_env::EvalEnv;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_summary::HelperSummary;
use crate::local_projection::{
    direct_bound_paths_from_expr_in_context, local_expression_facts_from_exprs,
};
use crate::output_path;
use crate::predicate::Predicate;

pub(crate) fn branch_guard_paths_for_expr(
    expr: &TemplateExpr,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BTreeSet<String> {
    let env = EvalEnv::from_helper_context(Some(bindings), current_dot);
    let mut branch_guard_paths = direct_bound_paths_from_expr_in_context(expr, &env);
    let local_facts = local_expression_facts_from_exprs(
        std::slice::from_ref(expr),
        local_bindings,
        &HashMap::new(),
        &HashMap::new(),
    );
    branch_guard_paths.extend(local_facts.source_paths);

    let nested = BoundHelperEnv::new(bindings, current_dot, context).summarize_calls_in_exprs(
        std::slice::from_ref(expr),
        local_bindings,
        seen,
    );
    branch_guard_paths.extend(dependency_paths_from_summary(&nested));
    branch_guard_paths
}

pub(crate) fn truthy_predicate_for_paths(paths: &BTreeSet<String>) -> Predicate {
    Predicate::all(paths.iter().cloned().map(Predicate::truthy_path).collect())
}

fn dependency_paths_from_summary(summary: &HelperSummary) -> BTreeSet<String> {
    let paths = summary
        .path_facts()
        .filter(|(_path, facts)| facts.is_dependency_relevant())
        .map(|(path, _facts)| path.to_string())
        .filter(|path| !path.trim().is_empty())
        .collect();
    remove_ancestor_paths(paths)
}

fn remove_ancestor_paths(paths: BTreeSet<String>) -> BTreeSet<String> {
    paths
        .iter()
        .filter(|path| !output_path::values_path_has_descendant(path, &paths))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "tests/helper_runtime_guards.rs"]
mod tests;
