use std::collections::BTreeSet;

use helm_schema_ast::DefineIndex;
use helm_schema_ir::{
    ContractIr, DependencyValuesRoot, ParsedDefines, SymbolicIrContext, SymbolicPolicy,
};
use helm_schema_k8s::LocalSchemaUniverse;

use super::local_crd_projection::collect_static_crd_universe;
use super::manifest_contract::{
    DefineCorpus, ManifestContractAnalysis, activation_predicate,
    collect_manifest_contract_for_chart, helm_enabled_guard_sets,
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
    corpus: &chart::LoadedChartCorpus,
    defines: &DefineIndex,
    values_roots: &ValuesRoots,
    kubernetes_version: Option<&str>,
) -> EngineResult<ChartAnalysis> {
    let parsed_defines = ParsedDefines::new(defines);
    let mut contract = ContractIr::default();
    let mut local_schema_universe = collect_static_crd_universe(charts, corpus)?;
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

    let define_corpus = DefineCorpus::build(charts, &parsed_defines);
    let dependency_global_ownership = chart::build_dependency_global_ownership(charts)?;
    for chart in charts {
        if chart.is_library {
            continue;
        }
        let symbolic_context = SymbolicIrContext::with_parsed_policy(
            &parsed_defines,
            SymbolicPolicy {
                chart_default_strings: values_roots
                    .string_defaults_for_prefix(&chart.values_prefix),
                kubernetes_version: kubernetes_version.map(str::to_string),
                static_root_strings: chart.static_root_strings.clone(),
            },
        );
        let optional_helpers = optional_dependency_helpers_for_chart(chart, charts, &define_corpus);
        let ManifestContractAnalysis {
            contract: manifest_contract,
            local_resource_schemas,
        } = collect_manifest_contract_for_chart(
            chart,
            corpus.chart(chart)?,
            &symbolic_context,
            &optional_helpers,
            &define_corpus,
        )?;
        contract.append(manifest_contract);
        for resource_schema in local_resource_schemas {
            local_schema_universe.insert_resource_schema(resource_schema);
        }
    }

    // Helm coalesces a values table for EVERY loaded dependency instance,
    // however deeply nested, aliased, or empty its defaults, and validates it
    // against that dependency's own schema.
    let dependency_roots = dependency_values_roots(charts, &mut contract)?;
    seed_top_level_values_yaml_keys(&mut contract, values_roots, &dependency_roots);
    for root in dependency_roots {
        contract.push_dependency_values_root(root);
    }

    Ok(ChartAnalysis {
        contract,
        local_schema_universe,
        shadowed_input_paths: dependency_global_ownership.shadowed_input_paths,
    })
}

/// Every dependency chart instance's values root, with the chart-tree facts
/// Helm's `type mismatch` assertions depend on.
///
/// A parent whose own defaults declare a non-table at an active listed
/// dependency's key aborts every render: Helm merges each active chart's
/// defaults with its dependencies' tables after pruning. That is recorded as
/// a terminal clause under the dependency's Helm enablement.
fn dependency_values_roots(
    charts: &[chart::ChartContext],
    contract: &mut ContractIr,
) -> EngineResult<Vec<DependencyValuesRoot>> {
    let own_values = charts
        .iter()
        .map(|chart| {
            Ok((
                chart.values_prefix.as_slice(),
                chart::chart_own_values(chart)?,
            ))
        })
        .collect::<EngineResult<std::collections::BTreeMap<_, _>>>()?;
    let by_prefix = charts
        .iter()
        .map(|chart| (chart.values_prefix.as_slice(), chart))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut roots = Vec::new();
    for chart in charts {
        let Some((key, parent_prefix)) = chart.values_prefix.split_last() else {
            continue;
        };
        let active = helm_enabled_guard_sets(&chart.dependency_activation_chain);
        let declared = own_values
            .get(parent_prefix)
            .and_then(serde_yaml::Value::as_mapping)
            .and_then(|mapping| mapping.get(key.as_str()));
        // Only a LISTED dependency keeps the parent's dependency metadata
        // non-nil, which is what makes Helm merge the parent's defaults with
        // its dependencies' tables.
        if chart.listed_dependency && declared.is_some_and(|value| !value.is_mapping()) {
            contract.add_terminal_fail_condition(activation_predicate(active.clone()));
        }
        // The pre-pruning pass keys nested dependencies by chart name; only
        // the analyzed chart's own dependencies carry their aliases there.
        let asserted_before_pruning = (2..=chart.values_prefix.len()).all(|depth| {
            chart.values_prefix.get(..depth).is_some_and(|prefix| {
                by_prefix.get(prefix).is_some_and(|edge| {
                    edge.static_root_strings
                        .get(&["Chart".to_string(), "Name".to_string()][..])
                        == prefix.last()
                })
            })
        });
        roots.push(DependencyValuesRoot {
            path: helm_schema_core::ValuesPath::from_segments(
                chart
                    .values_prefix
                    .iter()
                    .cloned()
                    .map(helm_schema_core::Segment::from),
            ),
            active,
            asserted_before_pruning,
            declared_by_parent: declared.is_some(),
        });
    }
    Ok(roots)
}
