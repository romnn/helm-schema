use std::collections::BTreeMap;

use crate::contract_sink::ContractUseContext;
use crate::document_projection::{
    DocumentOutput, collect_document_hole_context, collect_document_value_analysis,
};
use crate::helper_analysis::HelperOutputMeta;
use crate::helper_analysis_projection::helper_output_meta_from_analysis;

use super::SymbolicWalker;

impl SymbolicWalker<'_> {
    pub(super) fn helper_output_meta_for_text(
        &self,
        text: &str,
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = self
            .value_path_context()
            .local_alias_output_meta_for_text(text);
        let analysis = self.analyze_bound_helper_calls(text);
        for (path, meta) in helper_output_meta_from_analysis(&analysis) {
            out.entry(path).or_default().merge(meta);
        }
        out
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        let Ok(text) = node.utf8_text(self.source.as_bytes()) else {
            return;
        };

        self.inline_static_file_templates_from_helper_calls(text);

        let hole_context =
            collect_document_hole_context(self.source, &self.rendered_yaml, node, text);
        let kind = hole_context.kind;

        let helper_inlined = self.inline_exact_helper_call(text);

        let helper_analysis = if helper_inlined {
            None
        } else {
            Some(self.analyze_bound_helper_calls(text))
        };
        let value_path_context = self.value_path_context();
        let mut output_values = collect_document_value_analysis(
            text,
            kind,
            &value_path_context,
            &self.scope.locals().range_domains,
            &self.scope.locals().get_bindings,
            helper_analysis,
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

        let document_contract = {
            let guards = self.compatibility_guards();
            let projection_context = ContractUseContext::new(
                &guards,
                &self.scope.locals().chart_value_defaults,
                self.no_output_depth > 0,
            );
            DocumentOutput::new(hole_context, helper_inlined, output_values)
                .into_contract_ir(&projection_context)
        };
        self.contract.append(document_contract);
    }
}
