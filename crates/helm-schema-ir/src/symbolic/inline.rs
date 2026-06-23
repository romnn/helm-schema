use std::collections::{BTreeMap, HashSet};

use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::helper_arg_projection::bindings_for_helper_arg_with;
use crate::helper_inline::plan_exact_helper_inline_from_exprs;
use crate::static_file_template::{
    StaticFileTemplate, collect_template_requests_from_helper, literal_helper_calls_from_exprs,
};
use crate::tree_sitter_utils::parse_go_template;
use crate::{ContractUse, ValueKind, YamlPath};
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
        .with_inline_helpers_in_fragments(true)
        .with_chart_value_defaults(self.scope.locals().chart_value_defaults.clone());
        let contract = nested.run_contract(&tree);
        self.contract.append(contract);
    }

    pub(super) fn inline_exact_helper_call(&mut self, exprs: &[TemplateExpr]) -> bool {
        let helper_summary = self.summarize_bound_helper_calls_in_exprs(exprs);
        let Some(plan) = plan_exact_helper_inline_from_exprs(
            exprs,
            self.defines,
            &self.ir_context.inner.define_bodies,
            &self.inline_stack,
        ) else {
            return false;
        };

        let current_dot = self.current_dot_binding();
        let env = EvalEnv::from_helper_context(Some(&self.root_bindings), current_dot.as_ref());
        let bindings =
            bindings_for_helper_arg_with(plan.arg.as_ref(), Some(&self.root_bindings), |expr| {
                eval_expr(expr, &env)
                    .value
                    .map(|value| value.to_context_value())
            });
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
        .with_helper_values(bindings)
        .with_chart_value_defaults(self.scope.locals().chart_value_defaults.clone());
        let mut contract = nested.run_contract(&plan.tree);
        let helper_renders_output = helper_summary.has_document_value_facts();
        let suppress_roots = helper_summary.suppress_roots.clone();
        let mut helper_type_hints = BTreeMap::new();
        let mut inline_dependency_meta = BTreeMap::new();
        for (path, facts) in helper_summary.path_facts() {
            if !facts.type_hints.is_empty() {
                helper_type_hints.insert(path.to_string(), facts.type_hints.clone());
            }
            if let Some(meta) = facts.dependency_meta.as_ref()
                && !suppress_roots.contains(path)
            {
                inline_dependency_meta.insert(path.to_string(), meta.clone());
            }
        }
        if helper_renders_output {
            contract.extend_type_hints(helper_type_hints);
        }
        self.contract.append(contract);
        let outer_guards = self.contract_guards();
        for (value, meta) in inline_dependency_meta {
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
