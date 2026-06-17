use helm_schema_engine::{Guard, ResourceRef, YamlPath};
use helm_schema_k8s::{ChartLocalCrdSchemaProvider, K8sSchemaProvider};
use serde_json::json;
use vfs::VfsPath;

use super::analyze_charts;
use crate::chart;

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

    let ir_fact = collection
        .contract_schema_signals
        .value_path_facts
        .get(path)
        .unwrap_or_else(|| panic!("missing IR-derived fact for {path}"));
    assert!(
        ir_fact.all_render_uses_self_guarded,
        "IR-derived chart fact should stay self-guarded: {ir_fact:#?}"
    );

    Ok(())
}

#[test]
fn signoz_root_service_account_helper_is_reachable_for_type_hints() -> color_eyre::eyre::Result<()>
{
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
fn dependency_activation_guards_subchart_contract_uses() -> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc::indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies:
              - name: child
                alias: kid
                version: 0.1.0
                condition: kid.enabled, global.kidEnabled
                tags:
                  - observability
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "apiVersion: v2\nname: child\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        "enabled: true\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/configmap.yaml")?,
        r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: demo
data:
  enabled: "{{ .Values.enabled }}"
"#,
    )?;

    let discovery = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&discovery.charts, false)?;
    let collection = analyze_charts(&discovery.charts, &defines, false, None)?;
    let projection = collection.contract.project();
    let use_ = projection
        .uses()
        .iter()
        .find(|use_| {
            use_.source_expr == "kid.enabled"
                && use_.path.0 == ["data".to_string(), "enabled".to_string()]
        })
        .unwrap_or_else(|| panic!("missing guarded subchart use: {:?}", projection.uses()));

    assert!(
        use_.guards.contains(&Guard::Or {
            paths: vec![
                "global.kidEnabled".to_string(),
                "kid.enabled".to_string(),
                "tags.observability".to_string(),
            ]
        }),
        "expected Chart.yaml activation guard on subchart use, got {:?}",
        use_.guards
    );

    Ok(())
}

#[test]
fn literal_crd_template_populates_chart_local_schema_universe() -> color_eyre::eyre::Result<()> {
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

    let schema = provider.schema_fragment_for_resource_path(
        &resource,
        &YamlPath(vec!["spec".to_string(), "size".to_string()]),
    );

    assert_eq!(
        schema.map(|fragment| fragment.into_schema()),
        Some(json!({"type": "integer"}))
    );

    Ok(())
}

#[test]
fn templated_crd_template_populates_chart_local_schema_universe() -> color_eyre::eyre::Result<()> {
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
  name: {{ printf "%s.example.com" "widgets" }}
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

    let schema = provider.schema_fragment_for_resource_path(
        &resource,
        &YamlPath(vec!["spec".to_string(), "size".to_string()]),
    );

    assert_eq!(
        schema.map(|fragment| fragment.into_schema()),
        Some(json!({"type": "integer"}))
    );

    Ok(())
}
