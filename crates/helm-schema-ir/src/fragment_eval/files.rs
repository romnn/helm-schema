//! Nested-fragment inlining for static file templates: when an output
//! hole's helper call statically requests a `files/*` template render (a
//! `tpl (.Files.Get …)` chain inside the helper body), the referenced
//! file's CST evaluates as a nested fragment with the request's dot
//! binding. Request collection is shared with the pipeline
//! (`static_file_template`); nothing here re-derives it.
//!
//! The nested contributions merge at the include site — enclosing control
//! regions stamp their conditions onto the merged arms exactly like
//! locally-rendered content.

use helm_schema_ast::{TemplateExpr, parse_go_template};
use helm_schema_syntax::TemplatedDocument;

use crate::fragment_expr_eval::FragmentEvalContext;
use crate::static_file_template::{
    StaticTemplateProgram, StaticTemplateSource, collect_template_requests_from_exprs,
    collect_template_requests_from_helper, literal_helper_calls_from_exprs,
};
use helm_schema_core::{Guard, GuardValue, Predicate};

use super::domain::{AbstractFragment, Guarded};
use super::eval::{Interpreter, NodeView};
use crate::abstract_value::AbstractValue;

impl<'a> Interpreter<'a> {
    /// Resolve every static file template requested by the hole or its
    /// literal helper calls and evaluate the referenced files as nested
    /// fragments.
    pub(super) fn inline_static_file_fragments(
        &mut self,
        exprs: &[TemplateExpr],
    ) -> Guarded<AbstractFragment> {
        self.inline_static_templates(exprs).0
    }

    pub(super) fn inline_static_template_value(
        &mut self,
        exprs: &[TemplateExpr],
    ) -> Option<AbstractValue> {
        self.inline_static_templates(exprs).1
    }

    fn inline_static_templates(
        &mut self,
        exprs: &[TemplateExpr],
    ) -> (Guarded<AbstractFragment>, Option<AbstractValue>) {
        let mut out = Guarded::empty();
        let mut values = Vec::new();
        let direct_requests = {
            let context = FragmentEvalContext::new(self.db);
            let current_dot = self.current_dot_fragment();
            collect_template_requests_from_exprs(
                exprs,
                current_dot.as_ref(),
                &self.locals.fragment_values,
                &self.locals.output_meta,
                context,
            )
        };
        for request in direct_requests {
            let (fragment, value) = self.eval_static_template_program(&request);
            out.extend(fragment);
            values.extend(value);
        }
        for helper_call in literal_helper_calls_from_exprs(exprs) {
            let requests = {
                let context = FragmentEvalContext::new(self.db);
                let current_dot = self.current_dot_fragment();
                let mut seen = self.helper_seen.clone();
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
                let (fragment, value) = self.eval_static_template_program(&request);
                out.extend(fragment);
                values.extend(value);
            }
        }
        (out, AbstractValue::choice(values))
    }

    /// Evaluate one requested file as a nested fragment. The nested
    /// interpreter starts from the request's dot binding, inherits the
    /// ambient predicates (so its pathless reads carry the include site's
    /// guards, mirroring the current pipeline's seeded nested walk) and the
    /// chart-level default mutations observed so far; file-internal local
    /// state stays nested-only.
    fn eval_static_template_program(
        &mut self,
        request: &StaticTemplateProgram,
    ) -> (Guarded<AbstractFragment>, Option<AbstractValue>) {
        let db = self.db;
        let token: String;
        let source_path: &str;
        let source: &str;
        let selection_predicate: Option<Predicate>;
        let textual_program: bool;
        match &request.source {
            StaticTemplateSource::File { path } => {
                let Some(file_source) = db.file_source(path) else {
                    return (Guarded::empty(), None);
                };
                token = format!("file:{path}");
                source_path = path;
                source = file_source;
                selection_predicate = None;
                textual_program = false;
            }
            StaticTemplateSource::ValuesDefault { path, program } => {
                token = format!("values-default:{path}:{program}");
                source_path = path;
                source = program;
                selection_predicate = Some(Predicate::from(Guard::Eq {
                    path: path.clone(),
                    value: GuardValue::string(program),
                }));
                textual_program = true;
            }
            StaticTemplateSource::Constructed { program } => {
                token = format!("constructed:{program}");
                source_path = "@tpl";
                source = program;
                selection_predicate = None;
                textual_program = true;
            }
        }
        if self.inline_files.iter().any(|entry| entry == &token) {
            return (Guarded::empty(), None);
        }
        let Some(tree) = parse_go_template(source) else {
            return (Guarded::empty(), None);
        };
        let document = TemplatedDocument::parse_with_root(source, tree.root_node());
        let mut nested = Interpreter::for_source(source, Some(source_path), db, &tree, &document);
        nested.inline_files = self.inline_files.clone();
        nested.inline_files.push(token);
        nested.helper_scope = self.helper_scope || textual_program;
        nested.helper_seen = self.helper_seen.clone();
        nested
            .locals
            .set_chart_value_defaults(self.locals.chart_value_defaults.clone());
        nested.dot_stack.push(request.dot.clone());
        nested.active_predicates = self.active_predicates.clone();
        if let Some(predicate) = selection_predicate {
            nested.active_predicates.push(predicate);
        }
        let roots: Vec<NodeView<'_>> = document.roots().iter().map(NodeView::plain).collect();
        let contributions = nested.eval_node_list(&roots);
        for read in nested.reads {
            self.push_nested_read(read);
        }
        for (path, hints) in nested.type_hints {
            self.type_hints.entry(path).or_default().extend(hints);
        }
        for (path, hints) in nested.guarded_type_hints {
            self.guarded_type_hints
                .entry(path)
                .or_default()
                .extend(hints);
        }
        for (path, hints) in nested.fallback_type_hints {
            self.fallback_type_hints
                .entry(path)
                .or_default()
                .extend(hints);
        }
        for (path, hints) in nested.guarded_fallback_type_hints {
            self.guarded_fallback_type_hints
                .entry(path)
                .or_default()
                .extend(hints);
        }
        self.parsed_yaml_input_paths
            .extend(nested.parsed_yaml_input_paths);
        self.yaml_serialized_paths
            .extend(nested.yaml_serialized_paths);
        self.shape_erased_paths.extend(nested.shape_erased_paths);
        self.string_contract_paths
            .extend(nested.string_contract_paths);
        self.range_modes.merge(&nested.range_modes);
        for capture in nested.fail_conditions {
            if !self.fail_conditions.contains(&capture) {
                self.fail_conditions.push(capture);
            }
        }
        self.absorb_member_host_conversions(&nested.member_host_conversions);
        self.apply_root_set_mutations(
            &nested.root_set_mutations_observed,
            &nested.root_set_predicates_observed,
        );
        self.values_default_sources_observed
            .extend(nested.values_default_sources_observed);
        self.values_root_helper_includes_observed
            .extend(nested.values_root_helper_includes_observed);
        self.chart_defaults_observed
            .extend(nested.chart_defaults_observed);
        self.suppress_predicate_paths
            .extend(nested.suppress_predicate_paths);
        let fragment = contributions.assemble();
        if textual_program {
            let value = super::summary::projected_value(&fragment)
                .map(AbstractValue::require_rendered_source_presence);
            (Guarded::empty(), value)
        } else {
            (fragment, None)
        }
    }
}
