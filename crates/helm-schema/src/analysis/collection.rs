use std::collections::BTreeSet;

use helm_schema_ast::DefineIndex;
use helm_schema_ir::{ContractIr, SymbolicIrContext};
use helm_schema_k8s::LocalSchemaUniverse;

use super::local_crd_projection::collect_static_crd_universe;
use super::manifest_contract::{ManifestContractAnalysis, collect_manifest_contract_for_chart};
use super::values_seed::seed_top_level_values_yaml_keys;
use crate::chart;
use crate::error::EngineResult;
use crate::values_roots::ValuesRoots;

/// Contract and auxiliary signals collected from a chart tree.
pub(crate) struct ChartAnalysis {
    pub(crate) contract: ContractIr,
    pub(crate) local_schema_universe: LocalSchemaUniverse,
}

#[tracing::instrument(skip_all)]
pub(crate) fn analyze_charts(
    charts: &[chart::ChartContext],
    defines: &DefineIndex,
    include_tests: bool,
    values_roots: &ValuesRoots,
) -> EngineResult<ChartAnalysis> {
    let mut contract = ContractIr::default();
    let mut local_schema_universe = collect_static_crd_universe(charts)?;
    for chart in charts {
        for path in chart
            .dependency_activation
            .condition_paths
            .iter()
            .chain(chart.dependency_activation.tag_paths.iter())
        {
            let path = path.trim();
            if !path.is_empty() {
                contract.add_type_hint(path.to_string(), "boolean");
            }
        }
    }

    for chart in charts {
        if chart.is_library {
            continue;
        }
        let symbolic_context = SymbolicIrContext::with_chart_default_strings(
            defines,
            values_roots.string_defaults_for_prefix(&chart.values_prefix),
        );
        let ManifestContractAnalysis {
            contract: manifest_contract,
            local_resource_schemas,
        } = collect_manifest_contract_for_chart(chart, &symbolic_context, include_tests)?;
        contract.append(manifest_contract);
        for resource_schema in local_resource_schemas {
            local_schema_universe.insert_resource_schema(resource_schema);
        }
    }

    let dependency_root_paths = charts
        .iter()
        .filter_map(|chart| chart.values_prefix.first().cloned())
        .collect::<BTreeSet<_>>();
    seed_top_level_values_yaml_keys(&mut contract, values_roots, &dependency_root_paths);

    Ok(ChartAnalysis {
        contract,
        local_schema_universe,
    })
}
