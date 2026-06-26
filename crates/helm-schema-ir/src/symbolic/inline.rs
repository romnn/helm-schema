use std::collections::HashSet;

use crate::eval_env::EvalEnv;
use crate::expr_eval::{bindings_for_helper_arg_with, eval_expr, expr_literal_helper_call_callee};
use crate::static_file_template::{
    StaticFileTemplate, collect_template_requests_from_helper, literal_helper_calls_from_exprs,
};
use crate::{ContractUse, ValueKind, YamlPath};
use helm_schema_ast::TemplateExpr;
use helm_schema_ast::parse_go_template;

use super::SymbolicWalker;

impl SymbolicWalker<'_> {
    pub(super) fn inline_static_file_templates_from_helper_calls(
        &mut self,
        exprs: &[TemplateExpr],
    ) {
        for helper_call in literal_helper_calls_from_exprs(exprs) {
            let requests = {
                let context = self.fragment_eval_context();
                let current_dot = self.current_dot_fragment();
                let mut seen = HashSet::new();
                let helper_dot = helper_call.arg.as_ref().and_then(|arg| {
                    context.fragment_value_from_expr(
                        arg,
                        &self.scope.locals().fragment_values,
                        current_dot.as_ref(),
                        &mut seen,
                    )
                });
                collect_template_requests_from_helper(
                    &helper_call.name,
                    helper_dot.as_ref(),
                    context,
                )
            };
            for request in requests {
                self.inline_static_file_template(request);
            }
        }
    }

    fn inline_static_file_template(&mut self, request: StaticFileTemplate) {
        let token = format!("file:{}", request.path);
        if self.inline_stack.iter().any(|entry| entry == &token) {
            return;
        }
        let Some(src) = self.defines.get_file(&request.path) else {
            return;
        };
        let Some(tree) = parse_go_template(src) else {
            return;
        };

        let mut stack = self.inline_stack.clone();
        stack.push(token);
        let mut nested = SymbolicWalker::new_with_context(
            src,
            Some(request.path.as_str()),
            0,
            self.defines,
            self.ir_context.clone(),
        )
        .with_initial_predicates(self.scope.predicates().to_vec())
        .with_initial_dot_binding(request.dot)
        .with_inline_stack(stack)
        .with_chart_value_defaults(self.scope.locals().chart_value_defaults.clone());
        let contract = nested.run_contract(&tree);
        self.contract.append(contract);
    }

    pub(super) fn inline_exact_helper_call(&mut self, exprs: &[TemplateExpr]) -> bool {
        let [expr] = exprs else {
            return false;
        };
        let TemplateExpr::Call { args, .. } = expr else {
            return false;
        };
        let Some(name) = expr_literal_helper_call_callee(expr) else {
            return false;
        };
        if !crate::resource_identity::helper_body_defines_resource(name, self.defines) {
            return false;
        }
        let Some(body) = self.ir_context.inner.analysis_db.parsed_helper_body(name) else {
            return false;
        };
        let token = format!("define:{name}");
        if self.inline_stack.iter().any(|entry| entry == &token) {
            return false;
        };
        let helper_summary = self.summarize_bound_helper_calls_in_exprs(exprs);

        let current_dot = self.current_dot_binding();
        let env = EvalEnv::from_helper_context(Some(&self.root_bindings), current_dot.as_ref());
        let arg = args.get(1).cloned();
        let bindings =
            bindings_for_helper_arg_with(arg.as_ref(), Some(&self.root_bindings), |expr| {
                eval_expr(expr, &env)
                    .value
                    .map(|value| value.to_context_value())
            });
        let mut stack = self.inline_stack.clone();
        stack.push(token);
        let mut nested = SymbolicWalker::new_with_context(
            body.source,
            Some(body.source_path),
            body.body_offset,
            self.defines,
            self.ir_context.clone(),
        )
        .with_initial_predicates(self.scope.predicates().to_vec())
        .with_inline_stack(stack)
        .with_helper_values(bindings)
        .with_chart_value_defaults(self.scope.locals().chart_value_defaults.clone());
        let mut contract = nested.run_contract(&body.tree);
        let helper_renders_output = helper_summary.has_document_value_facts();
        let suppress_roots = helper_summary.suppress_roots;
        let helper_type_hints = helper_summary.type_hints;
        let inline_dependency_meta = helper_summary.dependency_meta;
        if helper_renders_output {
            contract.extend_type_hints(helper_type_hints);
        }
        self.contract.append(contract);
        let outer_guards = self.contract_guards();
        for (value, meta) in inline_dependency_meta {
            if suppress_roots.contains(&value) {
                continue;
            }
            for extra_guards in meta.contract_guard_sets(&value) {
                let mut guards = outer_guards.clone();
                for guard in extra_guards {
                    if !guards.contains(&guard) {
                        guards.push(guard);
                    }
                }
                self.contract.push(ContractUse::new(
                    value.clone(),
                    YamlPath(Vec::new()),
                    ValueKind::Scalar,
                    guards,
                    None,
                ));
            }
        }
        true
    }
}
