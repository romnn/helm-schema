use std::collections::BTreeSet;

use helm_schema_ast::DefineIndex;
use helm_schema_ir::{ContractIr, SymbolicIrContext, SymbolicPolicy};
use helm_schema_k8s::LocalSchemaUniverse;

use super::local_crd_projection::collect_static_crd_universe;
use super::manifest_contract::{
    DefineCorpus, ManifestContractAnalysis, collect_manifest_contract_for_chart,
    optional_dependency_helpers_for_chart,
};
use super::values_seed::seed_top_level_values_yaml_keys;
use crate::chart;
use crate::error::EngineResult;
use crate::values_roots::ValuesRoots;

/// Contract and auxiliary signals collected from a chart tree.
pub(crate) struct ChartAnalysis {
    pub(crate) contract: ContractIr,
    pub(crate) local_schema_universe: LocalSchemaUniverse,
    pub(crate) shadowed_input_paths: BTreeSet<String>,
}

#[tracing::instrument(skip_all)]
pub(crate) fn analyze_charts(
    charts: &[chart::ChartContext],
    defines: &DefineIndex,
    include_tests: bool,
    values_roots: &ValuesRoots,
    kubernetes_version: Option<&str>,
) -> EngineResult<ChartAnalysis> {
    let mut contract = ContractIr::default();
    if charts.iter().any(|chart| !chart.values_prefix.is_empty()) {
        // Helm accepts a root `global` value for dependency propagation even
        // when the root chart does not declare or read it. Keep the namespace
        // visible without assigning it a shape: a non-map source is valid and
        // simply skips injection into every child.
        contract.push_pathless_scalar("global");
    }
    let mut local_schema_universe = collect_static_crd_universe(charts)?;
    for chart in charts {
        for path in chart
            .dependency_activation_chain
            .iter()
            .flat_map(|level| level.condition_paths.iter().chain(level.tag_paths.iter()))
        {
            let path = path.trim();
            if !path.is_empty() {
                contract.add_type_hint(path.to_string(), "boolean");
            }
        }
    }

    let corpus = DefineCorpus::build(charts, defines);
    let dependency_global_ownership = chart::build_dependency_global_ownership(charts)?;
    for chart in charts {
        if chart.is_library {
            continue;
        }
        let symbolic_context = SymbolicIrContext::with_policy(
            defines,
            SymbolicPolicy {
                chart_default_strings: values_roots
                    .string_defaults_for_prefix(&chart.values_prefix),
                kubernetes_version: kubernetes_version.map(str::to_string),
                static_root_strings: chart.static_root_strings.clone(),
            },
        );
        let optional_helpers = optional_dependency_helpers_for_chart(chart, charts, &corpus);
        let ManifestContractAnalysis {
            contract: manifest_contract,
            local_resource_schemas,
        } = collect_manifest_contract_for_chart(
            chart,
            &symbolic_context,
            include_tests,
            &optional_helpers,
            &corpus,
        )?;
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
        shadowed_input_paths: dependency_global_ownership.shadowed_input_paths,
    })
}
