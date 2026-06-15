use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::fragment_binding::FragmentBinding;
use crate::helper_aware_expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::helper_binding::HelperBinding;
use crate::helper_binding_projection::{project_fragment_binding, project_helper_binding};

use super::FragmentEvalContext;

pub(super) fn eval_expr_with_bound_helpers(
    expr: &TemplateExpr,
    env: &EvalEnv,
    params: BoundHelperValueResolverParams<'_, '_, '_>,
) -> Option<AbstractValue> {
    let mut resolver = BoundHelperValueResolver { params };
    eval_expr_with_helper_calls(expr, env, &mut resolver)
}

pub(super) struct BoundHelperValueResolverParams<'a, 'context, 'seen> {
    pub(super) fragment_locals: &'a HashMap<String, FragmentBinding>,
    pub(super) outer: Option<&'a HashMap<String, HelperBinding>>,
    pub(super) current_dot: Option<&'a HelperBinding>,
    pub(super) context: FragmentEvalContext<'context>,
    pub(super) seen: &'seen mut HashSet<String>,
    pub(super) projection: HelperAnalysisProjection,
}

#[derive(Clone, Copy)]
pub(super) enum HelperAnalysisProjection {
    HelperBinding,
    FragmentBinding,
}

struct BoundHelperValueResolver<'a, 'context, 'seen> {
    params: BoundHelperValueResolverParams<'a, 'context, 'seen>,
}

impl HelperCallValueResolver for BoundHelperValueResolver<'_, '_, '_> {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        arg: Option<&TemplateExpr>,
    ) -> Option<AbstractValue> {
        let analysis = self
            .params
            .context
            .helper_summaries()
            .analyze_bound_helper_call(
                name,
                arg,
                self.params.outer,
                self.params.current_dot,
                self.params.fragment_locals,
                self.params.context,
                self.params.seen,
            );
        match self.params.projection {
            HelperAnalysisProjection::HelperBinding => project_helper_binding(analysis)
                .map(|binding| AbstractValue::from_helper_binding(&binding)),
            HelperAnalysisProjection::FragmentBinding => project_fragment_binding(analysis)
                .as_ref()
                .map(AbstractValue::from_fragment_binding),
        }
    }
}
