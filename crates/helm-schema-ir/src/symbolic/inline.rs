use std::collections::HashSet;

use crate::define_body_cache::parse_go_template;
use crate::expression_analysis::helper_bindings_for_arg;
use crate::helper_inline::plan_exact_helper_inline_from_exprs;
use crate::static_file_template::{
    StaticFileTemplate, collect_template_requests_from_helper, literal_helper_calls_from_exprs,
};
use helm_schema_ast::TemplateExpr;

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
                    context.fragment_binding_from_expr(
                        arg,
                        &self.scope.locals().fragment_bindings,
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
        .with_inline_helpers_in_fragments(true)
        .with_chart_value_defaults(self.scope.locals().chart_value_defaults.clone());
        let contract = nested.run_contract(&tree);
        self.contract.append(contract);
    }

    pub(super) fn inline_exact_helper_call(&mut self, exprs: &[TemplateExpr]) -> bool {
        let Some(plan) = plan_exact_helper_inline_from_exprs(
            exprs,
            self.defines,
            &self.ir_context.inner.define_bodies,
            &self.inline_stack,
        ) else {
            return false;
        };

        let current_dot = self.current_dot_binding();
        let bindings = helper_bindings_for_arg(
            plan.arg.as_ref(),
            Some(&self.root_bindings),
            current_dot.as_ref(),
        );
        let mut stack = self.inline_stack.clone();
        stack.push(plan.token);
        let mut nested = SymbolicWalker::new_with_context(
            plan.source,
            plan.source_path,
            plan.source_offset,
            self.defines,
            self.ir_context.clone(),
        )
        .with_initial_predicates(self.scope.predicates().to_vec())
        .with_inline_stack(stack)
        .with_inline_helpers_in_fragments(true)
        .with_helper_bindings(bindings)
        .with_chart_value_defaults(self.scope.locals().chart_value_defaults.clone());
        let contract = nested.run_contract(&plan.tree);
        self.contract.append(contract);
        true
    }
}
