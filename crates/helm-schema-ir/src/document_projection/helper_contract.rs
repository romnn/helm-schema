use std::collections::BTreeSet;

use crate::contract::ContractIr;
use crate::contract_sink::ContractUseContext;
use crate::{ValueKind, output_path};

use super::site::DocumentSite;
use super::value_analysis::DocumentHelperSummary;

pub(super) fn append_document_helper_contract_uses(
    helper: DocumentHelperSummary,
    encoded_output_values: &BTreeSet<String>,
    site: &DocumentSite,
    contract: &mut ContractIr,
    context: &ContractUseContext<'_>,
) {
    let structured_fragment_sources: BTreeSet<String> = helper
        .fragment_output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect();
    let mut helper_rendered_sources = structured_fragment_sources.clone();
    helper_rendered_sources.extend(helper.output_values.keys().cloned());
    let only_scalar_helper_outputs = helper.fragment_output_uses.is_empty();

    for (value, meta) in &helper.output_values {
        if structured_fragment_sources.contains(value) {
            continue;
        }
        let has_rendered_descendant =
            output_path::values_path_has_descendant(value, &helper_rendered_sources);
        for extra_guards in meta.contract_guard_sets(value) {
            let emit_kind = encoded_kind(site.kind(), encoded_output_values.contains(value));
            if only_scalar_helper_outputs
                && site.can_project_scalar_helper_to_caller_path()
                && !has_rendered_descendant
            {
                contract.push(site.contract_use_with_extra_provenance(
                    context,
                    value.clone(),
                    site.path().clone(),
                    emit_kind,
                    extra_guards,
                    &meta.provenance,
                ));
            } else {
                contract.push(context.pathless_contract_use_with_extra_provenance(
                    value.clone(),
                    ValueKind::Scalar,
                    &extra_guards,
                    &meta.provenance,
                ));
            }
        }
    }

    for output in helper.fragment_output_uses {
        let has_rendered_descendant =
            output_path::values_path_has_descendant(&output.source_expr, &helper_rendered_sources);
        for extra_guards in output.meta.contract_guard_sets(&output.source_expr) {
            let output_encoded =
                output.encoded || encoded_output_values.contains(&output.source_expr);
            let emit_kind = encoded_kind(output.kind, output_encoded);
            if site.can_project_structured_helper_to_caller_path() && !has_rendered_descendant {
                let emit_path =
                    output_path::append_relative_path(site.path(), &output.relative_path);
                contract.push(site.contract_use_with_extra_provenance(
                    context,
                    output.source_expr.clone(),
                    emit_path,
                    emit_kind,
                    extra_guards,
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

    for (value, meta) in helper.dependency_values {
        for extra_guards in meta.contract_guard_sets(&value) {
            contract.push(context.pathless_contract_use_with_extra_provenance(
                value.clone(),
                ValueKind::Scalar,
                &extra_guards,
                &meta.provenance,
            ));
        }
    }

    for value in helper.guard_values {
        contract.push(context.pathless_contract_use(value, ValueKind::Scalar, &[]));
    }

    contract.extend_type_hints(helper.type_hints);
}

fn encoded_kind(kind: ValueKind, encoded: bool) -> ValueKind {
    if encoded {
        ValueKind::PartialScalar
    } else {
        kind
    }
}
