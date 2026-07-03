//! Static-file-template inlining: when an output hole's helper call
//! statically requests a `files/*` template render (a `tpl (.Files.Get …)`
//! chain inside the helper body), the referenced file's CST evaluates as a
//! nested fragment with the request's dot binding, and its contributions
//! merge at the include site — enclosing control regions stamp their
//! conditions onto the merged arms exactly like locally-rendered content.
//! Request collection is shared with the current pipeline
//! (`static_file_template`); nothing here re-derives it.

use std::collections::HashSet;

use helm_schema_ast::{TemplateExpr, parse_go_template};
use helm_schema_syntax::TemplatedDocument;

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
        let mut nested = Interpreter::for_source(source, db, &tree, &document);
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
        contributions.assemble()
    }
}
