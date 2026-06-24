use std::collections::BTreeMap;

use helm_schema_ast::TemplateExpr;

use crate::SourceSpan;
use crate::contract_sink::ContractUseContext;
use crate::document_projection::{collect_document_site_context, document_output_contract};
use crate::helper_summary::HelperOutputMeta;

use super::SymbolicWalker;

impl SymbolicWalker<'_> {
    pub(super) fn helper_output_meta_for_exprs(
        &self,
        exprs: &[TemplateExpr],
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = self
            .value_path_context()
            .expression_output_effects(exprs)
            .local_output_meta;
        let analysis = self.summarize_bound_helper_calls_in_exprs(exprs);
        for (path, meta) in analysis.scalar_output_meta {
            out.entry(path).or_default().merge(meta);
        }
        for output in analysis.fragment_output_uses {
            out.entry(output.source_expr)
                .or_default()
                .merge(output.meta);
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

        let mut helper_summary = self.summarize_bound_helper_calls_in_exprs(exprs);
        // Stash chart-level `set X "K" (X.K | default V)` mutations discovered
        // in any helper called from this text. Subsequent contract emissions
        // in this walker attach `Guard::Default { path }` for matching reads,
        // which models that the helper's `set` has already run by the time
        // those reads are evaluated.
        let mut chart_value_defaults = helper_summary.take_chart_value_defaults();
        self.scope
            .locals_mut()
            .append_chart_value_defaults(&mut chart_value_defaults);

        let document_contract = {
            let value_path_context = self.value_path_context();
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
            document_output_contract(
                site_context,
                exprs,
                kind,
                &value_path_context,
                &self.scope.locals().range_domains,
                &self.scope.locals().get_bindings,
                helper_summary,
                &projection_context,
            )
        };
        self.contract.append(document_contract);
    }
}
