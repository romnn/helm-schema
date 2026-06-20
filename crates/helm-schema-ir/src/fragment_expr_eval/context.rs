use std::collections::{HashMap, HashSet};

use helm_schema_ast::{DefineIndex, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::define_body_cache::DefineBodyCache;
use crate::helper_summary::HelperSummaryCache;

#[derive(Clone, Copy)]
pub(crate) struct FragmentEvalContext<'a> {
    pub(crate) defines: &'a DefineIndex,
    pub(crate) define_bodies: &'a DefineBodyCache,
    helper_summaries: &'a HelperSummaryCache,
}

impl<'a> FragmentEvalContext<'a> {
    pub(crate) fn new(
        defines: &'a DefineIndex,
        define_bodies: &'a DefineBodyCache,
        helper_summaries: &'a HelperSummaryCache,
    ) -> Self {
        Self {
            defines,
            define_bodies,
            helper_summaries,
        }
    }

    pub(crate) fn helper_summaries(&self) -> &'a HelperSummaryCache {
        self.helper_summaries
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
