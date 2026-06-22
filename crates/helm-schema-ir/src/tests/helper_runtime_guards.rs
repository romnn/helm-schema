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
