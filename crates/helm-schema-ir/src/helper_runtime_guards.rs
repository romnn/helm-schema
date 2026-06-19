use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::bound_helper_env::BoundHelperEnv;
use crate::eval_env::EvalEnv;
use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_binding::HelperBinding;
use crate::helper_summary_projection::helper_summary_condition_paths;
use crate::local_projection::{
    direct_bound_paths_from_expr_in_context, local_bound_paths_from_expr,
};
use crate::predicate::Predicate;

pub(crate) fn branch_guard_paths_for_expr(
    expr: &TemplateExpr,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    local_bindings: &HashMap<String, FragmentBinding>,
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
    branch_guard_paths.extend(helper_summary_condition_paths(&nested));
    branch_guard_paths
}

pub(crate) fn truthy_predicate_for_paths(paths: &BTreeSet<String>) -> Predicate {
    Predicate::all(paths.iter().cloned().map(Predicate::truthy_path).collect())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use helm_schema_ast::DefineIndex;

    use crate::define_body_cache::DefineBodyCache;
    use crate::fragment_expr_eval::FragmentEvalContext;
    use crate::helper_binding::HelperBinding;
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
            &HashMap::<String, HelperBinding>::new(),
            None,
            &HashMap::new(),
            FragmentEvalContext::new(&defines, &define_bodies, &helper_summaries),
            &mut seen,
        );

        assert_eq!(
            paths,
            ["signoz.serviceAccount.create".to_string()]
                .into_iter()
                .collect(),
            "parsed control expr: {:?}",
            header.expr()
        );
    }
}
