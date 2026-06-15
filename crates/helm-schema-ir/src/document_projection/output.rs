use crate::contract::ContractIr;
use crate::contract_sink::ContractUseContext;
use crate::{Guard, ValueKind, YamlPath};

use super::helper_contract::append_document_helper_contract_uses;
use super::hole::DocumentHole;
use super::hole_context::DocumentHoleContext;
use super::value_analysis::DocumentValueAnalysis;

/// A rendered manifest output site discovered while interpreting a template.
pub(crate) struct DocumentOutput {
    hole: DocumentHole,
    analysis: DocumentValueAnalysis,
}

impl DocumentOutput {
    pub(crate) fn new(
        hole_context: DocumentHoleContext,
        helper_inlined: bool,
        analysis: DocumentValueAnalysis,
    ) -> Self {
        Self {
            hole: DocumentHole::new(hole_context, helper_inlined),
            analysis,
        }
    }

    pub(crate) fn append_to_contract(
        self,
        contract: &mut ContractIr,
        context: &ContractUseContext<'_>,
    ) {
        let hole = self.hole;
        let DocumentValueAnalysis {
            default_fallback_values,
            values,
            local_output_meta,
            bound_values,
            helper,
        } = self.analysis;

        for value in values {
            if helper.suppress_direct_values.contains(&value) {
                contract.push(hole.contract_use(
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
            let mut extra_guards: Vec<Guard> = Vec::new();
            if let Some(meta) = local_output_meta.get(&value) {
                extra_guards.extend(meta.compatibility_guards(&value));
            }
            if default_fallback_values.contains(&value) && !extra_guards.contains(&default_guard) {
                extra_guards.push(default_guard);
            }

            let emit_path = hole.direct_value_path(&value);
            let emit_kind = hole.direct_value_kind();
            contract.push(hole.contract_use(context, value, emit_path, emit_kind, extra_guards));
        }

        for value in bound_values {
            contract.push(hole.contract_use(
                context,
                value,
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                Vec::new(),
            ));
        }

        append_document_helper_contract_uses(helper, &hole, contract, context);
    }
}
