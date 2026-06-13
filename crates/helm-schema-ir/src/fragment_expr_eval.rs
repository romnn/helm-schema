use std::collections::{HashMap, HashSet};

use helm_schema_ast::{DefineIndex, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::binding::{FragmentBinding, HelperBinding};
use crate::define_body_cache::DefineBodyCache;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::helper_arg_projection::bindings_for_helper_arg_with;
use crate::helper_aware_expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::helper_summary::HelperSummaryCache;
use crate::template_expr_analysis::expr_contains_helper_call;
use crate::template_expr_cache::parse_expr_text;

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

    pub(crate) fn fragment_binding_from_expr(
        &self,
        expr: &TemplateExpr,
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        fragment_binding_from_expr(expr, locals, current_dot, *self, seen)
    }
}

pub(crate) fn fragment_binding_from_outer_expr(
    expr: &TemplateExpr,
    outer_locals: Option<&HashMap<String, FragmentBinding>>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> Option<FragmentBinding> {
    if matches!(expr, TemplateExpr::Variable(var) if var.is_empty())
        && let Some(bindings) = outer
    {
        return Some(FragmentBinding::Dict(
            bindings
                .iter()
                .map(|(key, binding)| {
                    (
                        key.clone(),
                        AbstractValue::from_helper_binding(binding)
                            .to_fragment_binding()
                            .unwrap_or(FragmentBinding::Unknown),
                    )
                })
                .collect(),
        ));
    }

    let env = EvalEnv::from_outer_fragment_expr_context(outer_locals, outer, current_dot);
    eval_expr(expr, &env)
        .value
        .as_ref()
        .and_then(AbstractValue::to_fragment_binding)
}

pub(crate) fn helper_binding_from_expr_with_fragment_locals(
    expr: &TemplateExpr,
    fragment_locals: &HashMap<String, FragmentBinding>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<HelperBinding> {
    let env =
        EvalEnv::from_helper_context_with_fragment_locals(outer, current_dot, fragment_locals);
    eval_expr_with_bound_helpers(
        expr,
        &env,
        BoundHelperValueResolverParams {
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
            projection: HelperAnalysisProjection::HelperBinding,
        },
    )
    .and_then(|value| value.to_helper_binding())
}

fn eval_expr_with_bound_helpers(
    expr: &TemplateExpr,
    env: &EvalEnv,
    params: BoundHelperValueResolverParams<'_, '_, '_>,
) -> Option<AbstractValue> {
    let mut resolver = BoundHelperValueResolver { params };
    eval_expr_with_helper_calls(expr, env, &mut resolver)
}

struct BoundHelperValueResolverParams<'a, 'context, 'seen> {
    fragment_locals: &'a HashMap<String, FragmentBinding>,
    outer: Option<&'a HashMap<String, HelperBinding>>,
    current_dot: Option<&'a HelperBinding>,
    context: FragmentEvalContext<'context>,
    seen: &'seen mut HashSet<String>,
    projection: HelperAnalysisProjection,
}

#[derive(Clone, Copy)]
enum HelperAnalysisProjection {
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
            HelperAnalysisProjection::HelperBinding => analysis
                .into_helper_binding()
                .map(|binding| AbstractValue::from_helper_binding(&binding)),
            HelperAnalysisProjection::FragmentBinding => analysis
                .into_fragment_binding()
                .as_ref()
                .map(AbstractValue::from_fragment_binding),
        }
    }
}

pub(crate) fn bindings_for_helper_arg_with_fragment_locals(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    fragment_locals: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HashMap<String, HelperBinding> {
    bindings_for_helper_arg_with(arg, outer, |expr| {
        helper_binding_from_expr_with_fragment_locals(
            expr,
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
        )
    })
}

pub(crate) fn fragment_binding_from_expr(
    expr: &TemplateExpr,
    locals: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let env = EvalEnv::from_fragment_context(locals, current_dot);
    let current_dot_helper = current_dot.and_then(FragmentBinding::to_helper_binding);
    eval_expr_with_bound_helpers(
        expr,
        &env,
        BoundHelperValueResolverParams {
            fragment_locals: locals,
            outer: None,
            current_dot: current_dot_helper.as_ref(),
            context,
            seen,
            projection: HelperAnalysisProjection::FragmentBinding,
        },
    )
    .and_then(|value| value.to_fragment_binding())
}

pub(crate) fn fragment_binding_from_text(
    text: &str,
    locals: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let mut bindings = Vec::new();
    for expr in parse_expr_text(text) {
        if let Some(binding) = context.fragment_binding_from_expr(&expr, locals, current_dot, seen)
        {
            bindings.push(binding);
        }
    }
    FragmentBinding::choice(bindings)
}

pub(crate) fn fragment_binding_from_text_with_helper_context(
    text: &str,
    fragment_locals: &HashMap<String, FragmentBinding>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
    let mut bindings = Vec::new();
    for expr in parse_expr_text(text) {
        if !expr_contains_helper_call(&expr)
            && let Some(binding) = fragment_binding_from_expr(
                &expr,
                fragment_locals,
                current_dot_fragment.as_ref(),
                context,
                seen,
            )
        {
            bindings.push(binding);
            continue;
        }
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
        ) {
            bindings.push(binding.to_fragment_binding());
            continue;
        }
        if let Some(binding) = fragment_binding_from_expr(
            &expr,
            fragment_locals,
            current_dot_fragment.as_ref(),
            context,
            seen,
        ) {
            bindings.push(binding);
        }
    }
    FragmentBinding::choice(bindings)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use helm_schema_ast::{TreeSitterParser, parse_action_expressions};

    use super::*;
    use crate::helper_summary::HelperSummaryCache;

    fn single_expr(action: &str) -> TemplateExpr {
        let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
        assert_eq!(exprs.len(), 1, "expected exactly one parsed expression");
        exprs.into_iter().next().expect("expression exists")
    }

    fn empty_context<'a>(
        defines: &'a DefineIndex,
        define_bodies: &'a DefineBodyCache,
        helper_summaries: &'a HelperSummaryCache,
    ) -> FragmentEvalContext<'a> {
        FragmentEvalContext::new(defines, define_bodies, helper_summaries)
    }

    fn helper_binding_from_fragment_locals(
        action: &str,
        fragment_locals: &HashMap<String, FragmentBinding>,
    ) -> Option<HelperBinding> {
        let expr = single_expr(action);
        let defines = DefineIndex::new();
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = empty_context(&defines, &define_bodies, &helper_summaries);
        let mut seen = HashSet::new();
        helper_binding_from_expr_with_fragment_locals(
            &expr,
            fragment_locals,
            None,
            None,
            context,
            &mut seen,
        )
    }

    fn context_local() -> HashMap<String, FragmentBinding> {
        HashMap::from([(
            "ctx".to_string(),
            FragmentBinding::Dict(BTreeMap::from([(
                "config".to_string(),
                FragmentBinding::ValuesPath("serviceAccount".to_string()),
            )])),
        )])
    }

    fn helper_context<'a>(
        defines: &'a DefineIndex,
        define_bodies: &'a DefineBodyCache,
        helper_summaries: &'a HelperSummaryCache,
    ) -> FragmentEvalContext<'a> {
        empty_context(defines, define_bodies, helper_summaries)
    }

    #[test]
    fn outer_expr_bare_dot_uses_root_bindings_as_current_context() {
        let expr = single_expr(".");
        let root_bindings = HashMap::from([(
            "Values".to_string(),
            HelperBinding::ValuesPath(String::new()),
        )]);

        assert_eq!(
            fragment_binding_from_outer_expr(&expr, None, Some(&root_bindings), None),
            Some(FragmentBinding::Dict(BTreeMap::from([(
                "Values".to_string(),
                FragmentBinding::ValuesRoot,
            )])))
        );
    }

    #[test]
    fn outer_expr_root_variable_uses_root_bindings_as_current_context() {
        let expr = single_expr("$");
        let root_bindings = HashMap::from([(
            "Values".to_string(),
            HelperBinding::ValuesPath(String::new()),
        )]);

        assert_eq!(
            fragment_binding_from_outer_expr(&expr, None, Some(&root_bindings), None),
            Some(FragmentBinding::Dict(BTreeMap::from([(
                "Values".to_string(),
                FragmentBinding::ValuesRoot,
            )])))
        );
    }

    #[test]
    fn outer_expr_fragment_local_selector_uses_shared_expression_eval() {
        let expr = single_expr(r#"dict "name" $ctx.config.name"#);
        let fragment_locals = context_local();

        assert_eq!(
            fragment_binding_from_outer_expr(&expr, Some(&fragment_locals), None, None),
            Some(FragmentBinding::Dict(BTreeMap::from([(
                "name".to_string(),
                FragmentBinding::ValuesPath("serviceAccount.name".to_string()),
            )])))
        );
    }

    #[test]
    fn helper_binding_fragment_local_selector_uses_shared_expression_eval() {
        let binding = helper_binding_from_fragment_locals(
            r#"$ctx.config.name | toYaml | fromYaml"#,
            &context_local(),
        );

        assert_eq!(
            binding,
            Some(HelperBinding::ValuesPath("serviceAccount.name".to_string()))
        );
    }

    #[test]
    fn helper_binding_fragment_local_dict_uses_shared_expression_eval() {
        let binding = helper_binding_from_fragment_locals(
            r#"dict "name" $ctx.config.name"#,
            &context_local(),
        );

        assert_eq!(
            binding,
            Some(HelperBinding::Dict(BTreeMap::from([(
                "name".to_string(),
                HelperBinding::ValuesPath("serviceAccount.name".to_string()),
            )])))
        );
    }

    #[test]
    fn helper_binding_fragment_local_index_uses_shared_expression_eval() {
        let binding =
            helper_binding_from_fragment_locals(r#"index $ctx.config "name""#, &context_local());

        assert_eq!(
            binding,
            Some(HelperBinding::ValuesPath("serviceAccount.name".to_string()))
        );
    }

    #[test]
    fn bound_helper_call_uses_single_value_resolver_for_helper_projection() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &TreeSitterParser,
                r#"{{- define "common.name" -}}{{ .Values.nameOverride }}{{- end -}}"#,
            )
            .expect("parse helper source");
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = helper_context(&defines, &define_bodies, &helper_summaries);
        let expr = single_expr(r#"include "common.name" ."#);
        let mut seen = HashSet::new();

        assert_eq!(
            helper_binding_from_expr_with_fragment_locals(
                &expr,
                &HashMap::new(),
                None,
                None,
                context,
                &mut seen,
            ),
            Some(HelperBinding::OutputSet(
                [("nameOverride".to_string(), Default::default())]
                    .into_iter()
                    .collect()
            )),
        );
    }

    #[test]
    fn bound_helper_call_uses_single_value_resolver_for_fragment_projection() {
        let mut defines = DefineIndex::new();
        defines
            .add_source(
                &TreeSitterParser,
                r#"{{- define "common.name" -}}{{ .Values.nameOverride }}{{- end -}}"#,
            )
            .expect("parse helper source");
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = helper_context(&defines, &define_bodies, &helper_summaries);
        let expr = single_expr(r#"include "common.name" ."#);
        let mut seen = HashSet::new();

        assert_eq!(
            fragment_binding_from_expr(&expr, &HashMap::new(), None, context, &mut seen),
            Some(FragmentBinding::OutputSet(
                ["nameOverride".to_string()].into_iter().collect()
            )),
        );
    }
}
