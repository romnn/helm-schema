use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::bound_helper_env::BoundHelperEnv;
use crate::eval_env::EvalEnv;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_summary::HelperPathEntry;
use crate::local_projection::{
    direct_bound_paths_from_expr_in_context, local_bound_paths_from_expr,
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
    branch_guard_paths.extend(local_bound_paths_from_expr(expr, local_bindings));

    let nested = BoundHelperEnv::new(bindings, current_dot, context).summarize_calls_in_exprs(
        std::slice::from_ref(expr),
        local_bindings,
        seen,
    );
    branch_guard_paths.extend(dependency_paths_from_entries(nested.into_path_entries()));
    branch_guard_paths
}

pub(crate) fn truthy_predicate_for_paths(paths: &BTreeSet<String>) -> Predicate {
    Predicate::all(paths.iter().cloned().map(Predicate::truthy_path).collect())
}

fn dependency_paths_from_entries(
    entries: impl IntoIterator<Item = HelperPathEntry>,
) -> BTreeSet<String> {
    let paths = entries
        .into_iter()
        .filter(HelperPathEntry::is_dependency_relevant)
        .map(|entry| entry.path)
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
mod tests {
    use std::collections::{HashMap, HashSet};
    use test_util::prelude::sim_assert_eq;

    use helm_schema_ast::DefineIndex;

    use crate::abstract_value::AbstractValue;
    use crate::define_body_cache::DefineBodyCache;
    use crate::fragment_expr_eval::FragmentEvalContext;
    use crate::helper_summary::HelperSummaryCache;
    use helm_schema_ast::TemplateHeader;

    use super::branch_guard_paths_for_expr;

    #[test]
    fn branch_guard_paths_include_direct_values_condition() {
        let header = TemplateHeader::parse_control(".Values.signoz.serviceAccount.create");
        let defines = DefineIndex::new();
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let mut seen = HashSet::new();
        let paths = branch_guard_paths_for_expr(
            header.expr(),
            &HashMap::<String, AbstractValue>::new(),
            None,
            &HashMap::new(),
            FragmentEvalContext::new(&defines, &define_bodies, &helper_summaries),
            &mut seen,
        );

        sim_assert_eq!(
            have: paths,
            want: ["signoz.serviceAccount.create".to_string()]
                .into_iter()
                .collect(),
            "parsed control expr: {:?}",
            header.expr()
        );
    }
}
