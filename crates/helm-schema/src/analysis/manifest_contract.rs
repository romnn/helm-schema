use std::io::Read;

use helm_schema_ast::{DefineIndex, contains_template_action};
use helm_schema_ir::{ContractIr, Guard, SymbolicIrContext};
use helm_schema_k8s::LocalResourceSchema;
use vfs::VfsPath;

use super::local_crd_projection::local_resource_schemas_from_template_source;
use crate::chart;
use crate::error::CliResult;

#[tracing::instrument(skip_all, fields(prefix_len = chart.values_prefix.len()))]
pub(crate) fn collect_manifest_contract_for_chart(
    chart: &chart::ChartContext,
    defines: &DefineIndex,
    symbolic_context: &SymbolicIrContext,
    include_tests: bool,
) -> CliResult<ManifestContractAnalysis> {
    let mut contract = ContractIr::default();
    let mut local_resource_schemas = Vec::new();
    let activation_guard_sets = chart_activation_guard_sets(&chart.dependency_activation);

    let manifests = chart::list_manifest_templates(&chart.chart_dir, include_tests)?;
    for path in manifests {
        let TemplateManifestAnalysis {
            contract: mut manifest_contract,
            local_resource_schemas: template_local_resource_schemas,
        } = collect_manifest_contract_for_template(&path, defines, symbolic_context)?;
        manifest_contract
            .map_value_paths(|path| chart::scope_values_path(path, &chart.values_prefix));
        manifest_contract =
            apply_chart_activation_guard_sets(manifest_contract, &activation_guard_sets);
        contract.append(manifest_contract);
        local_resource_schemas.extend(template_local_resource_schemas);
    }

    Ok(ManifestContractAnalysis {
        contract,
        local_resource_schemas,
    })
}

pub(crate) struct ManifestContractAnalysis {
    pub(crate) contract: ContractIr,
    pub(crate) local_resource_schemas: Vec<LocalResourceSchema>,
}

struct TemplateManifestAnalysis {
    contract: ContractIr,
    local_resource_schemas: Vec<LocalResourceSchema>,
}

#[tracing::instrument(skip_all)]
fn collect_manifest_contract_for_template(
    path: &VfsPath,
    defines: &DefineIndex,
    symbolic_context: &SymbolicIrContext,
) -> CliResult<TemplateManifestAnalysis> {
    let mut source = String::new();
    path.open_file()?.read_to_string(&mut source)?;
    let contract =
        symbolic_context.generate_contract_ir_for_source(&source, path.as_str(), defines);
    let local_resource_schemas = local_resource_schemas_from_template_source(
        &source,
        path.as_str(),
        contains_template_action(&source)?,
    )?;
    Ok(TemplateManifestAnalysis {
        contract,
        local_resource_schemas,
    })
}

fn apply_chart_activation_guard_sets(
    contract: ContractIr,
    activation_guard_sets: &[Vec<Guard>],
) -> ContractIr {
    if activation_guard_sets.is_empty() {
        return contract;
    }

    let mut activated_contract = ContractIr::default();
    for guards in activation_guard_sets {
        let mut guarded_contract = contract.clone();
        guarded_contract.append_guards_to_all_uses(guards);
        activated_contract.append(guarded_contract);
    }
    activated_contract
}

fn chart_activation_guard_sets(activation: &chart::ChartDependencyActivation) -> Vec<Vec<Guard>> {
    let condition_paths = normalized_ordered_paths(&activation.condition_paths);
    let tag_paths = normalized_sorted_paths(&activation.tag_paths);

    if condition_paths.is_empty() && tag_paths.is_empty() {
        return Vec::new();
    }

    let mut guard_sets = Vec::new();
    for (index, condition_path) in condition_paths.iter().enumerate() {
        let mut guards = absent_guards(&condition_paths[..index]);
        guards.push(Guard::Truthy {
            path: condition_path.clone(),
        });
        guard_sets.push(guards);
    }

    let mut fallback_guards = absent_guards(&condition_paths);
    if tag_paths.is_empty() {
        guard_sets.push(fallback_guards);
    } else {
        let mut tag_truthy_guards = fallback_guards.clone();
        tag_truthy_guards.push(truthy_tag_guard(&tag_paths));
        guard_sets.push(tag_truthy_guards);

        fallback_guards.extend(absent_guards(&tag_paths));
        guard_sets.push(fallback_guards);
    }

    guard_sets
}

fn normalized_ordered_paths(paths: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut normalized = Vec::new();
    for path in paths.iter().filter_map(|path| {
        let path = path.trim();
        (!path.is_empty()).then_some(path.to_string())
    }) {
        if seen.insert(path.clone()) {
            normalized.push(path);
        }
    }
    normalized
}

fn normalized_sorted_paths(paths: &[String]) -> Vec<String> {
    let mut normalized = paths
        .iter()
        .filter_map(|path| {
            let path = path.trim();
            (!path.is_empty()).then_some(path.to_string())
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn absent_guards(paths: &[String]) -> Vec<Guard> {
    paths
        .iter()
        .cloned()
        .map(|path| Guard::Absent { path })
        .collect()
}

fn truthy_tag_guard(tag_paths: &[String]) -> Guard {
    match tag_paths {
        [path] => Guard::Truthy { path: path.clone() },
        paths => Guard::Or {
            paths: paths.to_vec(),
        },
    }
}
