use std::collections::{BTreeMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::SourceSpan;
use crate::contract_sink::ContractUseContext;
use crate::document_projection::{
    DocumentOutput, collect_document_site_context, collect_document_value_analysis_from_exprs,
};
use crate::helper_summary::HelperOutputMeta;

use super::SymbolicWalker;

impl SymbolicWalker<'_> {
    pub(super) fn helper_output_meta_for_exprs(
        &self,
        exprs: &[TemplateExpr],
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = self
            .value_path_context()
            .local_alias_output_meta_for_exprs(exprs);
        let analysis = self
            .ir_context
            .inner
            .helper_summaries
            .summarize_bound_helper_calls_in_exprs(
                exprs,
                Some(&self.root_bindings),
                self.current_dot_binding().as_ref(),
                &self.scope.locals().fragment_values,
                self.fragment_eval_context(),
                &mut HashSet::new(),
            );
        for entry in analysis.into_path_entries() {
            if let Some(meta) = entry.output_meta {
                out.entry(entry.path.clone()).or_default().merge(meta);
            }
            for output in entry.fragment_output_uses {
                out.entry(output.source_expr)
                    .or_default()
                    .merge(output.meta);
            }
        }
        out
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn handle_output_node(
        &mut self,
        node: tree_sitter::Node<'_>,
        exprs: &[TemplateExpr],
    ) {
        self.inline_static_file_templates_from_helper_calls(exprs);

        let site_context =
            collect_document_site_context(self.source, &self.document_tracker, node, exprs);
        if site_context.in_yaml_comment {
            return;
        }
        let kind = site_context.kind;

        if kind == crate::ValueKind::Scalar {
            self.inline_exact_helper_call(exprs);
        }

        let helper_summary = Some(self.summarize_bound_helper_calls_in_exprs(exprs));
        let value_path_context = self.value_path_context();
        let mut output_values = collect_document_value_analysis_from_exprs(
            exprs,
            kind,
            &value_path_context,
            &self.scope.locals().range_domains,
            &self.scope.locals().get_bindings,
            helper_summary,
        );
        // Stash chart-level `set X "K" (X.K | default V)` mutations discovered
        // in any helper called from this text. Subsequent contract emissions
        // in this walker attach `Guard::Default { path }` for matching reads,
        // which models that the helper's `set` has already run by the time
        // those reads are evaluated.
        let mut chart_value_defaults = output_values.take_chart_value_defaults();
        self.scope
            .locals_mut()
            .append_chart_value_defaults(&mut chart_value_defaults);
        if output_values.is_empty() {
            return;
        }

        {
            let guards = self.contract_guards();
            let projection_context = ContractUseContext::new(
                &guards,
                &self.scope.locals().chart_value_defaults,
                self.no_output_depth > 0,
                self.source_path,
                Some(SourceSpan::new(
                    self.source_offset + site_context.source_span.start,
                    self.source_offset + site_context.source_span.end,
                )),
                self.provenance_helper_chain(),
            );
            DocumentOutput::new(site_context, output_values)
                .append_to_contract(&mut self.contract, &projection_context);
        }
    }
}
