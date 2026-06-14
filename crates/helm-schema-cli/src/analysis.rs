use helm_schema_ast::DefineIndex;
use helm_schema_ir::{ContractIr, ContractSchemaSignals, SymbolicIrContext};
use helm_schema_k8s::LocalSchemaUniverse;
use serde_yaml::Value as YamlValue;

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

fn seed_top_level_values_yaml_keys(contract: &mut ContractIr, values_yaml: Option<&str>) {
    let Some(values_yaml) = values_yaml else {
        return;
    };
    let Ok(doc) = serde_yaml::from_str::<YamlValue>(values_yaml) else {
        return;
    };
    let YamlValue::Mapping(mapping) = doc else {
        return;
    };

    for (key, _) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        contract.push_pathless_scalar(key.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_schema_ir::{ResourceRef, YamlPath};
    use helm_schema_k8s::{ChartLocalCrdSchemaProvider, K8sSchemaProvider};
    use serde_json::json;
    use vfs::VfsPath;

    #[test]
    fn subchart_helper_render_with_guard_surfaces_scoped_self_guarded_fact()
    -> color_eyre::eyre::Result<()> {
        let chart_dir = VfsPath::new(vfs::MemoryFS::new());

        test_util::write(
            &chart_dir.join("Chart.yaml")?,
            "apiVersion: v2\nname: root\nversion: 0.1.0\ndependencies:\n  - name: child\n    alias: kid\n    version: 0.1.0\n",
        )?;
        test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;
        test_util::write(
            &chart_dir.join("charts/child/Chart.yaml")?,
            "apiVersion: v2\nname: child\nversion: 0.1.0\n",
        )?;
        test_util::write(
            &chart_dir.join("charts/child/values.yaml")?,
            "controller:\n  ingressClassResource:\n    parameters: {}\n",
        )?;
        test_util::write(
            &chart_dir.join("charts/child/templates/_helpers.tpl")?,
            r#"{{- define "common.tplvalues.render" -}}
{{- .value | toYaml -}}
{{- end -}}
"#,
        )?;
        test_util::write(
            &chart_dir.join("charts/child/templates/ingressclass.yaml")?,
            r#"apiVersion: networking.k8s.io/v1
kind: IngressClass
spec:
  {{- with .Values.controller.ingressClassResource.parameters }}
  parameters: {{ include "common.tplvalues.render" (dict "value" . "context" $) | nindent 4 }}
  {{- end }}
"#,
        )?;

        let discovery = chart::discover_chart_contexts(&chart_dir)?;
        let defines = chart::build_define_index(&discovery.charts, false)?;
        let collection = analyze_charts(&discovery.charts, &defines, false, None)?;
        let path = "kid.controller.ingressClassResource.parameters";

        let ir_facts = collection.contract_schema_signals.chart_facts;
        let ir_fact = ir_facts
            .path_facts
            .get(path)
            .unwrap_or_else(|| panic!("missing IR-derived fact for {path}: {ir_facts:#?}"));
        assert!(
            ir_fact.all_render_uses_self_guarded,
            "IR-derived chart fact should stay self-guarded: {ir_fact:#?}"
        );

        Ok(())
    }

    #[test]
    fn signoz_root_service_account_helper_is_reachable_for_type_hints()
    -> color_eyre::eyre::Result<()> {
        let chart_dir = test_util::workspace_testdata()
            .join("charts")
            .join("signoz-signoz");
        let chart_dir_str = chart_dir.to_string_lossy().to_string();
        let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
        let discovery = chart::discover_chart_contexts(&chart_dir)?;
        let defines = chart::build_define_index(&discovery.charts, false)?;
        let collection = analyze_charts(&discovery.charts, &defines, false, None)?;
        let path = "alertmanager.serviceAccount.name";

        assert!(
            collection.template_evidence.type_hints.contains_key(path),
            "expected type hint for {path}; reachable={:?}; hints={:?}",
            collection
                .template_evidence
                .reachable_helpers_from_chart(&Vec::<String>::new()),
            collection
                .template_evidence
                .type_hints
                .keys()
                .collect::<Vec<_>>()
        );

        Ok(())
    }

    #[test]
    fn literal_crd_template_populates_chart_local_schema_universe() -> color_eyre::eyre::Result<()>
    {
        let chart_dir = VfsPath::new(vfs::MemoryFS::new());

        test_util::write(
            &chart_dir.join("Chart.yaml")?,
            "apiVersion: v2\nname: root\nversion: 0.1.0\n",
        )?;
        test_util::write(&chart_dir.join("values.yaml")?, "spec:\n  size: 1\n")?;
        test_util::write(
            &chart_dir.join("templates/crd.yaml")?,
            r#"apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: widgets.example.com
spec:
  group: example.com
  names:
    kind: Widget
    plural: widgets
  scope: Namespaced
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                size:
                  type: integer
"#,
        )?;
        test_util::write(
            &chart_dir.join("templates/widget.yaml")?,
            r#"apiVersion: example.com/v1
kind: Widget
metadata:
  name: demo
spec:
  size: {{ .Values.spec.size }}
"#,
        )?;

        let discovery = chart::discover_chart_contexts(&chart_dir)?;
        let defines = chart::build_define_index(&discovery.charts, false)?;
        let collection = analyze_charts(&discovery.charts, &defines, false, None)?;
        let provider = ChartLocalCrdSchemaProvider::new(collection.local_schema_universe);
        let resource = ResourceRef {
            api_version: "example.com/v1".to_string(),
            kind: "Widget".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };

        let schema = provider.schema_for_resource_path(
            &resource,
            &YamlPath(vec!["spec".to_string(), "size".to_string()]),
        );

        assert_eq!(schema, Some(json!({"type": "integer"})));

        Ok(())
    }
}
