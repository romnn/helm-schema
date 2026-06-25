use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::SourceSpan;
use crate::contract::ContractIr;
use crate::contract_sink::ContractUseContext;
use crate::document_projection::OutputSlot;
use crate::eval_effect::Effects;
use crate::helper_summary::{HelperFragmentOutputUse, HelperSummary};
use crate::{Guard, ValueKind, YamlPath, output_path};

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
            output_contract(output_slot, output_effects, helper, &projection_context)
        };
        self.contract.append(document_contract);
    }
}

fn output_contract(
    site: OutputSlot,
    mut output_effects: Effects,
    helper: HelperSummary,
    context: &ContractUseContext<'_>,
) -> ContractIr {
    let mut contract = ContractIr::default();
    if site.kind == ValueKind::Scalar {
        let all_values = output_effects.output_paths.clone();
        output_effects
            .output_paths
            .retain(|path| !output_path::values_path_has_descendant(path, &all_values));
    }

    if output_effects.output_paths.is_empty()
        && output_effects.bound_output_paths.is_empty()
        && !helper.has_document_value_facts()
    {
        return contract;
    }

    let mut suppress_direct_values = helper.dependency_relevant_paths();
    suppress_direct_values.extend(helper.suppress_roots.iter().cloned());

    let output_values = std::mem::take(&mut output_effects.output_paths);
    for value in output_values {
        if suppress_direct_values.contains(&value)
            || suppress_direct_values
                .iter()
                .any(|root| output_path::values_path_is_descendant(&value, root))
        {
            contract.push(context.contract_use(
                value,
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                &[],
            ));
            continue;
        }

        let default_guard = Guard::Default {
            path: value.clone(),
        };
        let provider_path_suppressed = output_effects.encoded_paths.contains(&value);
        let emit_path = site.direct_value_path(&value);
        let emit_kind = if provider_path_suppressed {
            ValueKind::PartialScalar
        } else {
            site.direct_value_kind()
        };
        let mut guard_sets = output_effects
            .local_output_meta
            .get(&value)
            .map(|meta| meta.contract_guard_sets(&value))
            .unwrap_or_else(|| vec![Vec::new()]);
        for extra_guards in &mut guard_sets {
            if output_effects.defaults.contains(&value) && !extra_guards.contains(&default_guard) {
                extra_guards.push(default_guard.clone());
            }
            contract.push(context.contract_use(
                value.clone(),
                emit_path.clone(),
                emit_kind,
                extra_guards,
            ));
        }
    }

    let bound_output_values = std::mem::take(&mut output_effects.bound_output_paths);
    for value in bound_output_values {
        contract.push(context.contract_use(value, YamlPath(Vec::new()), ValueKind::Scalar, &[]));
    }

    contract.extend_type_hints(output_effects.schema_type_hints());
    append_helper_contract_uses(
        &helper,
        &output_effects.encoded_paths,
        &site,
        &mut contract,
        context,
    );
    contract
}

fn append_helper_contract_uses(
    helper: &HelperSummary,
    encoded_output_values: &BTreeSet<String>,
    site: &OutputSlot,
    contract: &mut ContractIr,
    context: &ContractUseContext<'_>,
) {
    contract.extend_type_hints(helper.type_hints.clone());
    let helper_has_only_scalar_outputs = helper
        .output_uses
        .iter()
        .all(HelperFragmentOutputUse::is_scalar_summary_output);
    for output in helper
        .output_uses
        .iter()
        .filter(|output| output.is_scalar_summary_output())
    {
        let value = &output.source_expr;
        if helper.has_structured_fragment_source(value) {
            continue;
        }
        for extra_guards in output.meta.contract_guard_sets(value) {
            let emit_kind = encoded_kind(site.kind, encoded_output_values.contains(value));
            if helper_has_only_scalar_outputs
                && site.can_project_scalar_helper_to_caller_path()
                && !helper.has_rendered_source_descendant(value)
            {
                contract.push(context.contract_use_with_extra_provenance(
                    value.clone(),
                    site.path.clone(),
                    emit_kind,
                    &extra_guards,
                    &output.meta.provenance,
                ));
            } else {
                contract.push(context.pathless_contract_use_with_extra_provenance(
                    value.clone(),
                    ValueKind::Scalar,
                    &extra_guards,
                    &output.meta.provenance,
                ));
            }
        }
    }

    for output in helper
        .output_uses
        .iter()
        .filter(|output| output.is_structured_output())
    {
        append_fragment_output_contract_use(
            output,
            helper,
            encoded_output_values,
            site,
            contract,
            context,
        );
    }

    for (value, meta) in &helper.dependency_meta {
        let structured_scalar_guard_sets = structured_scalar_output_guard_sets(helper, value);
        for extra_guards in meta.contract_guard_sets(value) {
            if structured_scalar_guard_sets.contains(&extra_guards) {
                continue;
            }
            contract.push(context.pathless_contract_use_with_extra_provenance(
                value.clone(),
                ValueKind::Scalar,
                &extra_guards,
                &meta.provenance,
            ));
        }
    }

    for value in &helper.guard_paths {
        contract.push(context.pathless_contract_use(value.clone(), ValueKind::Scalar, &[]));
    }
}

fn structured_scalar_output_guard_sets(helper: &HelperSummary, value: &str) -> Vec<Vec<Guard>> {
    let mut guard_sets = Vec::new();
    for output in helper.output_uses.iter().filter(|output| {
        output.source_expr == value
            && output.kind == ValueKind::Scalar
            && !output.relative_path.0.is_empty()
    }) {
        for guards in output.meta.contract_guard_sets(value) {
            if !guard_sets.contains(&guards) {
                guard_sets.push(guards);
            }
        }
    }
    guard_sets
}

fn append_fragment_output_contract_use(
    output: &HelperFragmentOutputUse,
    helper: &HelperSummary,
    encoded_output_values: &BTreeSet<String>,
    site: &OutputSlot,
    contract: &mut ContractIr,
    context: &ContractUseContext<'_>,
) {
    for extra_guards in output.meta.contract_guard_sets(&output.source_expr) {
        let output_encoded = output.encoded || encoded_output_values.contains(&output.source_expr);
        let emit_kind = encoded_kind(output.kind, output_encoded);
        if site.can_project_structured_helper_to_caller_path()
            && !helper.has_rendered_source_descendant(&output.source_expr)
        {
            let emit_path = output_path::append_relative_path(&site.path, &output.relative_path);
            contract.push(context.contract_use_with_extra_provenance(
                output.source_expr.clone(),
                emit_path,
                emit_kind,
                &extra_guards,
                &output.meta.provenance,
            ));
        } else {
            contract.push(context.pathless_contract_use_with_extra_provenance(
                output.source_expr.clone(),
                emit_kind,
                &extra_guards,
                &output.meta.provenance,
            ));
        }
    }
}

fn encoded_kind(kind: ValueKind, encoded: bool) -> ValueKind {
    if encoded {
        ValueKind::PartialScalar
    } else {
        kind
    }
}
