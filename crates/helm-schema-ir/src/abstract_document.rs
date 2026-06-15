use crate::abstract_document_hole::AbstractDocumentHole;
use crate::contract::ContractIr;
use crate::contract_sink::ContractUseContext;
use crate::document_helper_contract::append_document_helper_contract_uses;
use crate::document_hole_context::DocumentHoleContext;
use crate::document_value_analysis::DocumentValueAnalysis;
use crate::{Guard, ValueKind, YamlPath};

/// A rendered manifest output site discovered while interpreting a template.
///
/// This is still a compatibility-era document artifact: it records the
/// structural position of one rendered hole and lowers through a private
/// document projection before producing contract uses. Keeping that projection
/// behind a document-shaped type gives the next A4 steps a single place to
/// attach richer contract facts before final DTO projection.
pub(crate) struct AbstractDocumentOutput {
    hole: AbstractDocumentHole,
    analysis: DocumentValueAnalysis,
}

impl AbstractDocumentOutput {
    pub(crate) fn new(
        hole_context: DocumentHoleContext,
        helper_inlined: bool,
        analysis: DocumentValueAnalysis,
    ) -> Self {
        Self {
            hole: AbstractDocumentHole::new(hole_context, helper_inlined),
            analysis,
        }
    }

    pub(crate) fn into_contract_ir(self, context: &ContractUseContext<'_>) -> ContractIr {
        let mut contract = ContractIr::default();
        self.append_contract_uses(&mut contract, context);
        contract
    }

    fn append_contract_uses(self, contract: &mut ContractIr, context: &ContractUseContext<'_>) {
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
