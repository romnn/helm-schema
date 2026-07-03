//! Nested-fragment inlining: static file templates and resource-defining
//! helper bodies both evaluate as nested fragments at their include site.
//!
//! Static files: when an output hole's helper call statically requests a
//! `files/*` template render (a `tpl (.Files.Get …)` chain inside the helper
//! body), the referenced file's CST evaluates as a nested fragment with the
//! request's dot binding. Request collection is shared with the pipeline
//! (`static_file_template`); nothing here re-derives it.
//!
//! Resource helpers: a helper whose body structurally defines a manifest
//! (`kind:`/`apiVersion:` headers) evaluates exactly, so the body's own
//! resource identity scopes its placed rows.
//!
//! In both cases the nested contributions merge at the include site —
//! enclosing control regions stamp their conditions onto the merged arms
//! exactly like locally-rendered content.

use std::collections::HashSet;

use helm_schema_ast::{TemplateExpr, parse_go_template};
use helm_schema_syntax::TemplatedDocument;

use crate::eval_env::EvalEnv;
use crate::expr_eval::{bindings_for_helper_arg_with, eval_expr, expr_literal_helper_call_callee};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::static_file_template::{
    StaticFileTemplate, collect_template_requests_from_helper, literal_helper_calls_from_exprs,
};

use super::domain::{AbstractFragment, Guarded};
use super::eval::{Interpreter, NodeView};

impl<'a> Interpreter<'a> {
    /// Resolve every static file template requested by the hole's literal
    /// helper calls and evaluate the referenced files as nested fragments.
    pub(super) fn inline_static_file_fragments(
        &mut self,
        exprs: &[TemplateExpr],
    ) -> Guarded<AbstractFragment> {
        let mut out = Guarded::empty();
        for helper_call in literal_helper_calls_from_exprs(exprs) {
            let requests = {
                let context = FragmentEvalContext::new(self.db);
                let current_dot = self.current_dot_fragment();
                let mut seen = HashSet::new();
                let helper_dot = helper_call.arg.as_ref().and_then(|arg| {
                    context.fragment_value_from_expr(
                        arg,
                        &self.locals.fragment_values,
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
                out.extend(self.eval_static_file_fragment(&request));
            }
        }
        out
    }

    /// Evaluate one requested file as a nested fragment. The nested
    /// interpreter starts from the request's dot binding, inherits the
    /// ambient predicates (so its pathless reads carry the include site's
    /// guards, mirroring the current pipeline's seeded nested walk) and the
    /// chart-level default mutations observed so far; file-internal local
    /// state stays nested-only.
    fn eval_static_file_fragment(
        &mut self,
        request: &StaticFileTemplate,
    ) -> Guarded<AbstractFragment> {
        let token = format!("file:{}", request.path);
        if self.inline_files.iter().any(|entry| entry == &token) {
            return Guarded::empty();
        }
        let db = self.db;
        let Some(source) = db.file_source(&request.path) else {
            return Guarded::empty();
        };
        let Some(tree) = parse_go_template(source) else {
            return Guarded::empty();
        };
        let document = TemplatedDocument::parse_with_root(source, tree.root_node());
        let mut nested = Interpreter::for_source(source, Some(&request.path), db, &tree, &document);
        nested.inline_files = self.inline_files.clone();
        nested.inline_files.push(token);
        nested
            .locals
            .set_chart_value_defaults(self.locals.chart_value_defaults.clone());
        nested.dot_stack.push(request.dot.clone());
        nested.active_predicates = self.active_predicates.clone();
        let roots: Vec<NodeView<'_>> = document.roots().iter().map(NodeView::plain).collect();
        let contributions = nested.eval_node_list(&roots);
        for read in nested.reads {
            if !self.reads.contains(&read) {
                self.reads.push(read);
            }
        }
        for (path, hints) in nested.type_hints {
            self.type_hints.entry(path).or_default().extend(hints);
        }
        contributions.assemble()
    }

    /// Evaluate a resource-defining helper call as a nested fragment: a
    /// single literal `include`/`template` call whose body structurally
    /// declares a manifest evaluates exactly over the body's own CST, with
    /// the call's dict/list argument as the nested root bindings. Returns
    /// `None` when the hole is not such a call (the value/summary lowering
    /// handles it instead).
    pub(super) fn inline_resource_helper_fragment(
        &mut self,
        exprs: &[TemplateExpr],
    ) -> Option<Guarded<AbstractFragment>> {
        let [expr] = exprs else {
            return None;
        };
        let name = expr_literal_helper_call_callee(expr)?.to_string();
        if !crate::resource_identity::helper_body_defines_resource(&name, self.db) {
            return None;
        }
        let token = format!("define:{name}");
        if self.inline_files.iter().any(|entry| entry == &token) {
            return None;
        }
        let body = self.db.parsed_helper_body(&name)?;
        let helm_schema_ast::TemplateExpr::Call { args, .. } = expr.deparen() else {
            return None;
        };
        let current_dot = self.current_dot_binding();
        let env = EvalEnv::from_helper_context(Some(&self.root_bindings), current_dot.as_ref());
        let bindings =
            bindings_for_helper_arg_with(args.get(1), Some(&self.root_bindings), |expr| {
                eval_expr(expr, &env)
                    .value
                    .map(|value| value.to_context_value())
            })
            .bindings;

        let document = TemplatedDocument::parse_with_root(body.source, body.tree.root_node());
        let mut nested = Interpreter::for_source(
            body.source,
            Some(body.source_path),
            self.db,
            &body.tree,
            &document,
        );
        nested.source_offset = body.body_offset;
        nested.inline_files = self.inline_files.clone();
        nested.inline_files.push(token);
        nested
            .locals
            .set_chart_value_defaults(self.locals.chart_value_defaults.clone());
        nested.root_bindings = bindings;
        nested.active_predicates = self.active_predicates.clone();
        let roots: Vec<NodeView<'_>> = document.roots().iter().map(NodeView::plain).collect();
        let contributions = nested.eval_node_list(&roots);
        for read in nested.reads {
            if !self.reads.contains(&read) {
                self.reads.push(read);
            }
        }
        for (path, hints) in nested.type_hints {
            self.type_hints.entry(path).or_default().extend(hints);
        }
        Some(contributions.assemble())
    }
}
