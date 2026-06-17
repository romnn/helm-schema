use std::collections::{BTreeSet, HashMap, HashSet};

use crate::bound_helper_env::BoundHelperEnv;
use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_binding::HelperBinding;
use crate::helper_summary_projection::helper_summary_condition_paths;
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
    let nested = BoundHelperEnv::new(bindings, current_dot, context).summarize_calls(
        text,
        local_bindings,
        seen,
    );
    branch_guard_paths.extend(helper_summary_condition_paths(&nested));
    branch_guard_paths
}

pub(crate) fn truthy_predicate_for_paths(paths: &BTreeSet<String>) -> Predicate {
    Predicate::all(paths.iter().cloned().map(Predicate::truthy_path).collect())
}
