use std::collections::{HashMap, HashSet};

use helm_schema_ast::{DefineIndex, Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::binding::{FragmentBinding, HelperBinding};
use crate::define_body_cache::DefineBodyCache;
use crate::eval_env::EvalEnv;
use crate::helper_aware_expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::helper_call_analyzer::HelperCallAnalyzer;
use crate::template_expr_analysis::{expr_contains_helper_call, is_merge_function};
use crate::template_expr_cache::parse_expr_text;

#[derive(Clone, Copy)]
pub(crate) struct FragmentEvalContext<'a> {
    pub(crate) defines: &'a DefineIndex,
    pub(crate) define_bodies: &'a DefineBodyCache,
    helper_call_analyzer: &'a dyn HelperCallAnalyzer,
}

impl<'a> FragmentEvalContext<'a> {
    pub(crate) fn new(
        defines: &'a DefineIndex,
        define_bodies: &'a DefineBodyCache,
        helper_call_analyzer: &'a dyn HelperCallAnalyzer,
    ) -> Self {
        Self {
            defines,
            define_bodies,
            helper_call_analyzer,
        }
    }

    pub(crate) fn helper_call_analyzer(&self) -> &'a dyn HelperCallAnalyzer {
        self.helper_call_analyzer
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
    let mut resolver = HelperBindingResolver {
        fragment_locals,
        outer,
        current_dot,
        context,
        seen,
    };
    eval_expr_with_helper_calls(expr, &env, &mut resolver)
        .and_then(|value| value.to_helper_binding())
}

pub(crate) fn bindings_for_helper_arg_with_fragment_locals(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    fragment_locals: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HashMap<String, HelperBinding> {
    let Some(arg) = arg else {
        return HashMap::new();
    };

    match arg {
        TemplateExpr::Parenthesized(inner) => bindings_for_helper_arg_with_fragment_locals(
            Some(inner),
            outer,
            current_dot,
            fragment_locals,
            context,
            seen,
        ),
        TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Call { function, args } if function == "dict" => {
            let mut bindings = HashMap::new();
            let mut index = 0usize;
            while index + 1 < args.len() {
                let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                    &args[index]
                else {
                    index += 1;
                    continue;
                };
                let binding = helper_binding_from_expr_with_fragment_locals(
                    &args[index + 1],
                    fragment_locals,
                    outer,
                    current_dot,
                    context,
                    seen,
                )
                .unwrap_or(HelperBinding::Unknown);
                bindings.insert(key.clone(), binding);
                index += 2;
            }
            bindings
        }
        TemplateExpr::Call { function, args } if is_merge_function(function) => {
            let mut merged = HashMap::new();
            for arg in args {
                match helper_binding_from_expr_with_fragment_locals(
                    arg,
                    fragment_locals,
                    outer,
                    current_dot,
                    context,
                    seen,
                ) {
                    Some(HelperBinding::Dict(map)) => {
                        for (key, value) in map {
                            merged.insert(key, value);
                        }
                    }
                    Some(HelperBinding::RootContext) => {
                        if let Some(outer) = outer {
                            for (key, value) in outer {
                                merged.insert(key.clone(), value.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            merged
        }
        _ => HashMap::new(),
    }
}

pub(crate) fn fragment_binding_from_expr(
    expr: &TemplateExpr,
    locals: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let env = EvalEnv::from_fragment_context(locals, current_dot);
    let mut resolver = FragmentBindingResolver {
        locals,
        current_dot,
        context,
        seen,
    };
    eval_expr_with_helper_calls(expr, &env, &mut resolver)
        .and_then(|value| value.to_fragment_binding())
}

struct HelperBindingResolver<'a, 'context, 'seen> {
    fragment_locals: &'a HashMap<String, FragmentBinding>,
    outer: Option<&'a HashMap<String, HelperBinding>>,
    current_dot: Option<&'a HelperBinding>,
    context: FragmentEvalContext<'context>,
    seen: &'seen mut HashSet<String>,
}

impl HelperCallValueResolver for HelperBindingResolver<'_, '_, '_> {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        arg: Option<&TemplateExpr>,
    ) -> Option<AbstractValue> {
        let analysis = self
            .context
            .helper_call_analyzer()
            .analyze_bound_helper_call(
                name,
                arg,
                self.outer,
                self.current_dot,
                self.fragment_locals,
                self.context,
                self.seen,
            );
        analysis
            .into_helper_binding()
            .map(|binding| AbstractValue::from_helper_binding(&binding))
    }
}

struct FragmentBindingResolver<'a, 'context, 'seen> {
    locals: &'a HashMap<String, FragmentBinding>,
    current_dot: Option<&'a FragmentBinding>,
    context: FragmentEvalContext<'context>,
    seen: &'seen mut HashSet<String>,
}

impl HelperCallValueResolver for FragmentBindingResolver<'_, '_, '_> {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        arg: Option<&TemplateExpr>,
    ) -> Option<AbstractValue> {
        let current_dot_helper = self
            .current_dot
            .and_then(FragmentBinding::to_helper_binding);
        let analysis = self
            .context
            .helper_call_analyzer()
            .analyze_bound_helper_call(
                name,
                arg,
                None,
                current_dot_helper.as_ref(),
                self.locals,
                self.context,
                self.seen,
            );
        analysis
            .into_fragment_binding()
            .as_ref()
            .map(AbstractValue::from_fragment_binding)
    }
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

    use helm_schema_ast::parse_action_expressions;

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

    #[test]
    fn helper_binding_fragment_local_selector_uses_abstract_eval() {
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
    fn helper_binding_fragment_local_dict_uses_abstract_eval() {
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
    fn helper_binding_fragment_local_index_uses_abstract_eval() {
        let binding =
            helper_binding_from_fragment_locals(r#"index $ctx.config "name""#, &context_local());

        assert_eq!(
            binding,
            Some(HelperBinding::ValuesPath("serviceAccount.name".to_string()))
        );
    }
}
