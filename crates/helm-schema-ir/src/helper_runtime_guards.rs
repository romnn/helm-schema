use std::collections::{BTreeSet, HashMap, HashSet};

use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_analysis_projection::bound_helper_condition_paths;
use crate::helper_binding::HelperBinding;
use crate::local_projection::{
    direct_bound_paths_from_text_in_context, local_bound_paths_from_text,
};
use crate::predicate::Predicate;

pub(crate) fn branch_guard_paths(
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    local_bindings: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BTreeSet<String> {
    let mut branch_guard_paths =
        direct_bound_paths_from_text_in_context(text, bindings, current_dot);
    branch_guard_paths.extend(local_bound_paths_from_text(text, local_bindings));
    let nested = context.helper_summaries().analyze_bound_helper_calls(
        text,
        Some(bindings),
        current_dot,
        local_bindings,
        context,
        seen,
    );
    branch_guard_paths.extend(bound_helper_condition_paths(&nested));
    branch_guard_paths
}

pub(crate) fn truthy_predicate_for_paths(paths: &BTreeSet<String>) -> Predicate {
    Predicate::all(paths.iter().cloned().map(Predicate::truthy_path).collect())
}
