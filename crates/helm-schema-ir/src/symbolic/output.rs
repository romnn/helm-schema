use helm_schema_ast::TemplateExpr;

use crate::SourceSpan;
use crate::contract_sink::ContractUseContext;
use crate::document_projection::document_output_contract;

use super::SymbolicWalker;

impl SymbolicWalker<'_> {
    #[tracing::instrument(skip_all)]
    pub(super) fn handle_output_node(
        &mut self,
        node: tree_sitter::Node<'_>,
        exprs: &[TemplateExpr],
    ) {
        self.inline_static_file_templates_from_helper_calls(exprs);

        let output_slot = self.document_tracker.output_slot_for_action(node);
        if output_slot.is_yaml_comment() {
            return;
        }

        if output_slot.kind == crate::ValueKind::Scalar {
            self.inline_exact_helper_call(exprs);
        }

        let output_effects = self.value_path_context().expression_output_effects(exprs);
        let mut helper = self.summarize_bound_helper_calls_in_exprs(exprs);
        // Stash chart-level `set X "K" (X.K | default V)` mutations discovered
        // in any helper called from this text. Subsequent contract emissions
        // in this walker attach `Guard::Default { path }` for matching reads,
        // which models that the helper's `set` has already run by the time
        // those reads are evaluated.
        let mut chart_value_defaults = helper.take_chart_value_defaults();
        self.scope
            .locals_mut()
            .append_chart_value_defaults(&mut chart_value_defaults);

        let document_contract = {
            let guards = self.contract_guards();
            let projection_context = ContractUseContext::new(
                &guards,
                &self.scope.locals().chart_value_defaults,
                self.no_output_depth > 0,
                output_slot.resource.clone(),
                self.source_path,
                Some(SourceSpan::new(
                    self.source_offset + node.start_byte(),
                    self.source_offset + node.end_byte(),
                )),
                self.provenance_helper_chain(),
            );
            document_output_contract(output_slot, output_effects, helper, &projection_context)
        };
        self.contract.append(document_contract);
    }
}
