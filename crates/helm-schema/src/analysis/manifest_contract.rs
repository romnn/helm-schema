use std::io::Read;

use helm_schema_engine::helpers::DefineIndex;
use helm_schema_engine::parse::contains_template_action;
use helm_schema_engine::{ContractIr, Guard, SymbolicIrContext};
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
    let activation_guards = chart_activation_guards(&chart.dependency_activation);

    let manifests = chart::list_manifest_templates(&chart.chart_dir, include_tests)?;
    for path in manifests {
        let TemplateManifestAnalysis {
            contract: mut manifest_contract,
            local_resource_schemas: template_local_resource_schemas,
        } = collect_manifest_contract_for_template(&path, defines, symbolic_context)?;
        manifest_contract
            .map_value_paths(|path| chart::scope_values_path(path, &chart.values_prefix));
        manifest_contract.append_guards_to_all_uses(&activation_guards);
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

fn chart_activation_guards(activation: &chart::ChartDependencyActivation) -> Vec<Guard> {
    let activation_paths = chart_activation_paths(activation);

    match activation_paths.as_slice() {
        [] => Vec::new(),
        [path] => vec![Guard::Truthy { path: path.clone() }],
        _ => vec![Guard::Or {
            paths: activation_paths,
        }],
    }
}

fn chart_activation_paths(activation: &chart::ChartDependencyActivation) -> Vec<String> {
    let mut activation_paths = activation
        .condition_paths
        .iter()
        .chain(activation.tag_paths.iter())
        .filter_map(|path| {
            let path = path.trim();
            (!path.is_empty()).then_some(path.to_string())
        })
        .collect::<Vec<_>>();

    activation_paths.sort();
    activation_paths.dedup();
    activation_paths
}
