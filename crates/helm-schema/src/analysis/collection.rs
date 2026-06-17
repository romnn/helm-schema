use helm_schema_engine::helpers::DefineIndex;
use helm_schema_engine::{ContractIr, ContractSchemaSignals, SymbolicIrContext};
use helm_schema_k8s::LocalSchemaUniverse;

use super::manifest_contract::{ManifestContractAnalysis, collect_manifest_contract_for_chart};
use super::values_seed::seed_top_level_values_yaml_keys;
use crate::chart;
use crate::chart_evidence::{ChartTemplateEvidence, collect_chart_template_evidence};
use crate::error::CliResult;

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
            local_resource_schemas,
        } = collect_manifest_contract_for_chart(chart, defines, &symbolic_context, include_tests)?;
        contract.append(manifest_contract);
        for resource_schema in local_resource_schemas {
            local_schema_universe.insert_resource_schema(resource_schema);
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
