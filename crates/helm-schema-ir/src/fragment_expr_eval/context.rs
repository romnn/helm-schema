use std::collections::{BTreeMap, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::eval_env::{EvalEnv, LocalBinding};
use crate::helper_meta::HelperOutputMeta;

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
        self,
        expr: &TemplateExpr,
        locals: &HashMap<String, LocalBinding>,
        current_dot: Option<&AbstractValue>,
        seen: &mut HashSet<String>,
    ) -> Option<AbstractValue> {
        self.fragment_value_from_expr_with_meta(expr, locals, &HashMap::new(), current_dot, seen)
    }

    pub(crate) fn fragment_value_from_expr_with_meta(
        self,
        expr: &TemplateExpr,
        locals: &HashMap<String, LocalBinding>,
        local_output_meta: &HashMap<
            String,
            BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>,
        >,
        current_dot: Option<&AbstractValue>,
        seen: &mut HashSet<String>,
    ) -> Option<AbstractValue> {
        let mut env = EvalEnv::from_fragment_context(
            locals,
            current_dot,
            crate::eval_env::BindingEvaluationMode::Direct,
            self.analysis_db.predicate_memo(),
        );
        env.local_output_meta = local_output_meta.clone();
        let current_dot_helper = current_dot.map(AbstractValue::to_context_value);
        let result = eval_expr_result_with_bound_helpers(
            expr,
            &env,
            BoundHelperValueResolverParams {
                outer: None,
                current_dot: current_dot_helper.as_ref(),
                context: self,
                seen,
            },
        );
        let mut output_meta = result.effects.local_output_meta.clone();
        for path in &result.effects.yaml_serialized_paths {
            output_meta.entry(path.clone()).or_default().yaml_serialized = true;
        }
        result
            .value
            .and_then(AbstractValue::without_widened)
            .map(|value| value.with_output_meta(&output_meta))
            .map(|value| value.to_context_value())
    }
}
