use std::collections::BTreeSet;

use crate::contract::ContractIr;
use crate::contract_sink::ContractUseContext;
use crate::{Guard, ValueKind, YamlPath};

use super::helper_contract::append_document_helper_contract_uses;
use super::site::DocumentSite;
use super::site_context::DocumentSiteContext;
use super::value_analysis::DocumentValueAnalysis;

/// A rendered manifest output site discovered while interpreting a template.
pub(crate) struct DocumentOutput {
    site: DocumentSite,
    analysis: DocumentValueAnalysis,
}

impl DocumentOutput {
    pub(crate) fn new(
        site_context: DocumentSiteContext,
        helper_inlined: bool,
        analysis: DocumentValueAnalysis,
    ) -> Self {
        Self {
            site: DocumentSite::new(site_context, helper_inlined),
            analysis,
        }
    }

    pub(crate) fn append_to_contract(
        self,
        contract: &mut ContractIr,
        context: &ContractUseContext<'_>,
    ) {
        let site = self.site;
        let DocumentValueAnalysis {
            default_fallback_values,
            values,
            type_hints,
            local_output_meta,
            bound_values,
            helper,
        } = self.analysis;

        for value in values {
            if helper.suppress_direct_values.contains(&value)
                || suppresses_direct_descendant(&helper.suppress_direct_values, &value)
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
            let emit_path = site.direct_value_path(&value);
            let emit_kind = site.direct_value_kind();
            let mut guard_sets = local_output_meta
                .get(&value)
                .map(|meta| meta.contract_guard_sets(&value))
                .unwrap_or_else(|| vec![Vec::new()]);
            for extra_guards in &mut guard_sets {
                if default_fallback_values.contains(&value)
                    && !extra_guards.contains(&default_guard)
                {
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

        contract.extend_type_hints(type_hints);
        append_document_helper_contract_uses(helper, &site, contract, context);
    }
}

fn suppresses_direct_descendant(suppressed_roots: &BTreeSet<String>, value_path: &str) -> bool {
    suppressed_roots.iter().any(|root| {
        value_path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('.'))
    })
}
