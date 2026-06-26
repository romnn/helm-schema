use std::collections::{HashMap, HashSet};
use test_util::prelude::sim_assert_eq;

use helm_schema_ast::DefineIndex;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::fragment_expr_eval::FragmentEvalContext;
use helm_schema_ast::TemplateHeader;

use super::branch_guard_paths_for_expr;

#[test]
fn branch_guard_paths_include_direct_values_condition() {
    let header = TemplateHeader::parse_control(".Values.signoz.serviceAccount.create");
    let defines = DefineIndex::new();
    let analysis_db = IrAnalysisDb::new(&defines);
    let mut seen = HashSet::new();
    let paths = branch_guard_paths_for_expr(
        header.expr(),
        &HashMap::<String, AbstractValue>::new(),
        None,
        &HashMap::new(),
        FragmentEvalContext::new(&analysis_db),
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
