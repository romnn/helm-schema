use helm_schema_ast::DefineIndex;
use helm_schema_ir::{ContractIr, ContractSchemaSignals, SymbolicIrContext};
use helm_schema_k8s::LocalSchemaUniverse;

use super::values_seed::seed_top_level_values_yaml_keys;
use crate::chart;
use crate::chart_evidence::{ChartTemplateEvidence, collect_chart_template_evidence};
use crate::error::CliResult;
use crate::manifest_analysis::{ManifestContractAnalysis, collect_manifest_contract_for_chart};

/// Contract and auxiliary signals collected from a chart tree.
pub(crate) struct ChartAnalysis {
    pub(crate) contract_schema_signals: ContractSchemaSignals,
    pub(crate) template_evidence: ChartTemplateEvidence,
    pub(crate) local_schema_universe: LocalSchemaUniverse,
}

#[tracing::instrument(skip_all)]
pub(crate) fn analyze_charts(
    charts: &[chart::ChartContext],
    defines: &DefineIndex,
    include_tests: bool,
    values_yaml: Option<&str>,
) -> CliResult<ChartAnalysis> {
    let mut contract = ContractIr::default();
    let mut local_schema_universe = chart::collect_static_crd_universe(charts)?;
    let symbolic_context = SymbolicIrContext::new(defines);

    for chart in charts {
        if chart.is_library {
            continue;
        }
        let ManifestContractAnalysis {
            contract: manifest_contract,
            literal_crd_documents,
        } = collect_manifest_contract_for_chart(chart, defines, &symbolic_context, include_tests)?;
        contract.append(manifest_contract);
        for document in literal_crd_documents {
            local_schema_universe.insert_crd_document(document);
        }
    }

    let template_evidence = collect_chart_template_evidence(charts, include_tests)?;

    seed_top_level_values_yaml_keys(&mut contract, values_yaml);

    let contract_schema_signals = contract.into_schema_signals();

    Ok(ChartAnalysis {
        contract_schema_signals,
        template_evidence,
        local_schema_universe,
    })
}
