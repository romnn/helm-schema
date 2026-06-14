use std::io::Read;

use helm_schema_ast::{DefineIndex, contains_template_action};
use helm_schema_ir::{ContractIr, SymbolicIrContext};
use serde::Deserialize;
use serde_json::Value;
use vfs::VfsPath;

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
    let mut literal_crd_documents = Vec::new();

    let manifests = chart::list_manifest_templates(&chart.chart_dir, include_tests)?;
    for path in manifests {
        let TemplateManifestAnalysis {
            contract: mut manifest_contract,
            literal_crd_documents: template_literal_crd_documents,
        } = collect_manifest_contract_for_template(&path, defines, symbolic_context)?;
        manifest_contract
            .map_value_paths(|path| chart::scope_values_path(path, &chart.values_prefix));
        contract.append(manifest_contract);
        literal_crd_documents.extend(template_literal_crd_documents);
    }

    Ok(ManifestContractAnalysis {
        contract,
        literal_crd_documents,
    })
}

pub(crate) struct ManifestContractAnalysis {
    pub(crate) contract: ContractIr,
    pub(crate) literal_crd_documents: Vec<Value>,
}

struct TemplateManifestAnalysis {
    contract: ContractIr,
    literal_crd_documents: Vec<Value>,
}

#[tracing::instrument(skip_all)]
fn collect_manifest_contract_for_template(
    path: &VfsPath,
    defines: &DefineIndex,
    symbolic_context: &SymbolicIrContext,
) -> CliResult<TemplateManifestAnalysis> {
    let mut source = String::new();
    path.open_file()?.read_to_string(&mut source)?;
    let contract = symbolic_context.generate_contract_ir(&source, defines);
    let literal_crd_documents =
        literal_crd_documents_from_template(&source, contains_template_action(&source)?)?;
    Ok(TemplateManifestAnalysis {
        contract,
        literal_crd_documents,
    })
}

fn literal_crd_documents_from_template(
    source: &str,
    contains_template_action: bool,
) -> CliResult<Vec<Value>> {
    if contains_template_action {
        return Ok(Vec::new());
    }

    let mut documents = Vec::new();
    for document in serde_yaml::Deserializer::from_str(source) {
        let document = Value::deserialize(document)?;
        if !document.is_null() {
            documents.push(document);
        }
    }
    Ok(documents)
}
