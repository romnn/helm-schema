use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::binding::{FragmentBinding, HelperBinding};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_analysis::BoundHelperAnalysis;

pub(crate) trait HelperCallAnalyzer {
    fn analyze_bound_helper_calls(
        &self,
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis;

    fn analyze_bound_helper_call(
        &self,
        name: &str,
        arg: Option<&TemplateExpr>,
        outer_bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis;
}
