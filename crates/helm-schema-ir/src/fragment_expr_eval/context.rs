use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::eval_env::EvalEnv;

use super::bound_helper_resolver::{
    BoundHelperValueResolverParams, eval_expr_result_with_bound_helpers,
};

#[derive(Clone, Copy)]
pub(crate) struct FragmentEvalContext<'a> {
    pub(crate) analysis_db: &'a IrAnalysisDb,
}

impl<'a> FragmentEvalContext<'a> {
    pub(crate) fn new(analysis_db: &'a IrAnalysisDb) -> Self {
        Self { analysis_db }
    }

    pub(crate) fn fragment_value_from_expr(
        &self,
        expr: &TemplateExpr,
        locals: &HashMap<String, AbstractValue>,
        current_dot: Option<&AbstractValue>,
        seen: &mut HashSet<String>,
    ) -> Option<AbstractValue> {
        let env = EvalEnv::from_fragment_context(locals, current_dot);
        let current_dot_helper = current_dot.map(AbstractValue::to_context_value);
        eval_expr_result_with_bound_helpers(
            expr,
            &env,
            BoundHelperValueResolverParams {
                fragment_locals: locals,
                outer: None,
                current_dot: current_dot_helper.as_ref(),
                context: *self,
                seen,
            },
        )
        .value
        .and_then(AbstractValue::without_widened)
        .map(|value| value.to_context_value())
    }
}
