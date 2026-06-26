use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;

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
        super::fragment_value_from_expr(expr, locals, current_dot, *self, seen)
    }
}
