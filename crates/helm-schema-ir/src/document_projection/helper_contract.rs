use std::collections::{BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::ValueKind;
use crate::bound_value_analysis::{GetBinding, extract_bound_values_from_exprs};
use crate::contract::ContractIr;
use crate::contract_sink::ContractUseContext;
use crate::helper_summary::HelperSummary;
use crate::value_path_context::ValuePathContext;
use crate::{Guard, YamlPath, output_path};

use super::site_context::DocumentSiteContext;

pub(crate) fn document_output_contract(
    site: DocumentSiteContext,
    exprs: &[TemplateExpr],
    kind: ValueKind,
    value_path_context: &ValuePathContext<'_>,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
    helper: HelperSummary,
    context: &ContractUseContext<'_>,
) -> ContractIr {
    let mut contract = ContractIr::default();
    let mut output_effects = value_path_context.expression_output_effects(exprs);
    if kind == ValueKind::Scalar {
        let all_values = output_effects.output_paths.clone();
        output_effects
            .output_paths
            .retain(|path| !output_path::values_path_has_descendant(path, &all_values));
    }

    let bound_values = extract_bound_values_from_exprs(exprs, range_domains, get_bindings);

    if output_effects.output_paths.is_empty()
        && bound_values.is_empty()
        && !helper.has_document_value_facts()
    {
        return contract;
    }

    let suppress_roots = helper.suppress_roots.clone();
    let suppress_direct_values = suppress_direct_values_for_helper(&helper, suppress_roots);

    let output_values = std::mem::take(&mut output_effects.output_paths);
    for value in output_values {
        if suppress_direct_values.contains(&value)
            || suppresses_direct_descendant(&suppress_direct_values, &value)
        {
            contract.push(site.contract_use(
                context,
                value,
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                Vec::new(),
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
            contract.push(site.contract_use(
                context,
                value.clone(),
                emit_path.clone(),
                emit_kind,
                extra_guards.clone(),
            ));
        }
    }

    for value in bound_values {
        contract.push(site.contract_use(
            context,
            value,
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
        ));
    }

    contract.extend_type_hints(output_effects.schema_type_hints());
    append_document_helper_contract_uses(
        &helper,
        &output_effects.encoded_paths,
        &site,
        &mut contract,
        context,
    );
    contract
}

fn append_document_helper_contract_uses(
    helper: &HelperSummary,
    encoded_output_values: &BTreeSet<String>,
    site: &DocumentSiteContext,
    contract: &mut ContractIr,
    context: &ContractUseContext<'_>,
) {
    contract.extend_type_hints(helper.type_hints());
    let fragment_output_uses = helper.fragment_output_uses();
    let helper_has_only_scalar_outputs = fragment_output_uses.is_empty();
    for (value, meta) in helper.scalar_output_meta() {
        if !helper.has_structured_fragment_source(&value) {
            for extra_guards in meta.contract_guard_sets(&value) {
                let emit_kind = encoded_kind(site.kind, encoded_output_values.contains(&value));
                if helper_has_only_scalar_outputs
                    && site.can_project_scalar_helper_to_caller_path()
                    && !helper.has_rendered_source_descendant(&value)
                {
                    contract.push(site.contract_use_with_extra_provenance(
                        context,
                        value.clone(),
                        site.path.clone(),
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
    }

    for output in fragment_output_uses {
        append_fragment_output_contract_use(
            output,
            helper,
            encoded_output_values,
            site,
            contract,
            context,
        );
    }

    for (value, meta) in helper.dependency_meta() {
        for extra_guards in meta.contract_guard_sets(&value) {
            contract.push(context.pathless_contract_use_with_extra_provenance(
                value.clone(),
                ValueKind::Scalar,
                &extra_guards,
                &meta.provenance,
            ));
        }
    }

    for value in helper.guard_paths() {
        contract.push(context.pathless_contract_use(value, ValueKind::Scalar, &[]));
    }
}

fn append_fragment_output_contract_use(
    output: crate::helper_summary::HelperFragmentOutputUse,
    helper: &HelperSummary,
    encoded_output_values: &BTreeSet<String>,
    site: &DocumentSiteContext,
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

fn suppress_direct_values_for_helper(
    helper: &HelperSummary,
    suppress_roots: BTreeSet<String>,
) -> BTreeSet<String> {
    let mut suppress_direct_values = BTreeSet::new();
    for (path, facts) in helper.path_facts() {
        if facts.is_dependency_relevant() {
            suppress_direct_values.insert(path.to_string());
        }
    }

    let all_dependency_values = suppress_direct_values.clone();
    suppress_direct_values
        .retain(|path| !output_path::values_path_has_descendant(path, &all_dependency_values));
    suppress_direct_values.extend(suppress_roots);
    suppress_direct_values
}

fn encoded_kind(kind: ValueKind, encoded: bool) -> ValueKind {
    if encoded {
        ValueKind::PartialScalar
    } else {
        kind
    }
}

fn suppresses_direct_descendant(suppressed_roots: &BTreeSet<String>, value_path: &str) -> bool {
    suppressed_roots.iter().any(|root| {
        value_path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('.'))
    })
}

#[cfg(test)]
#[path = "../tests/document_projection/helper_contract.rs"]
mod tests;
