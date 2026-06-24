use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};

use super::FragmentEvalContext;

pub(super) fn eval_expr_result_with_bound_helpers(
    expr: &TemplateExpr,
    env: &EvalEnv,
    params: BoundHelperValueResolverParams<'_, '_, '_>,
) -> EvalResult {
    let mut resolver = BoundHelperValueResolver { params };
    eval_expr_with_helper_calls(expr, env, &mut resolver)
}

pub(super) struct BoundHelperValueResolverParams<'a, 'context, 'seen> {
    pub(super) fragment_locals: &'a HashMap<String, AbstractValue>,
    pub(super) outer: Option<&'a HashMap<String, AbstractValue>>,
    pub(super) current_dot: Option<&'a AbstractValue>,
    pub(super) context: FragmentEvalContext<'context>,
    pub(super) seen: &'seen mut HashSet<String>,
    pub(super) projection: HelperAnalysisProjection,
}

#[derive(Clone, Copy)]
pub(super) enum HelperAnalysisProjection {
    HelperValue,
    FragmentValue,
}

struct BoundHelperValueResolver<'a, 'context, 'seen> {
    params: BoundHelperValueResolverParams<'a, 'context, 'seen>,
}

impl HelperCallValueResolver for BoundHelperValueResolver<'_, '_, '_> {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        arg: Option<&TemplateExpr>,
    ) -> Option<EvalResult> {
        self.params.context.define_bodies.source(name)?;
        if self.params.seen.contains(name) {
            return Some(EvalResult::none());
        }
        let analysis = self
            .params
            .context
            .helper_summaries()
            .summarize_bound_helper_call(
                name,
                arg,
                self.params.outer,
                self.params.current_dot,
                self.params.fragment_locals,
                self.params.context,
                self.params.seen,
            );
        let value = match self.params.projection {
            HelperAnalysisProjection::HelperValue => analysis.clone().project_helper_value(),
            HelperAnalysisProjection::FragmentValue => analysis.clone().project_fragment_value(),
        };
        let mut effects = Effects::default();
        effects.helper_summary = analysis;
        Some(EvalResult::with_effects(value, effects))
    }
}
