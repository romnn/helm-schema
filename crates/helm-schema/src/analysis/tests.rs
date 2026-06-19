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
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(None),
    )?;
    let path = "kid.controller.ingressClassResource.parameters";

    let ir_fact = collection
        .contract_schema_signals()
        .evidence_for(path)
        .map(|evidence| evidence.facts)
        .unwrap_or_else(|| panic!("missing IR-derived fact for {path}"));
    assert!(
        ir_fact.all_render_uses_self_guarded,
        "IR-derived chart fact should stay self-guarded: {ir_fact:#?}"
    );

    Ok(())
}

#[test]
fn signoz_root_service_account_helper_type_hint_flows_into_contract_schema_signals()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("signoz-signoz");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let discovery = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&discovery.charts, false)?;
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(None),
    )?;
    let path = "clickhouse.zookeeper.nameOverride";

    assert!(
        collection
            .contract_schema_signals()
            .evidence_for(path)
            .is_some_and(|evidence| evidence.type_hints.contains("string")),
        "expected structural contract type hint for {path}; contract_hints={:?}",
        collection
            .contract_schema_signals()
            .schema_evidence_by_value_path(),
    );

    Ok(())
}

#[test]
fn signoz_clickhouse_operator_service_account_name_keeps_helper_and_else_branch_guards()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("signoz-signoz");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let discovery = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&discovery.charts, false)?;
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(None),
    )?;
    let projection = collection.contract.clone().project();
    let path = "clickhouse.clickhouseOperator.serviceAccount.name";
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        uses.iter().any(|use_| {
            use_.guards.iter().any(|guard| {
                matches!(
                    guard,
                    Guard::Truthy { path }
                    if path == "clickhouse.clickhouseOperator.serviceAccount.create"
                )
            })
        }),
        "expected a create=true helper-backed branch for {path}; uses={uses:#?}"
    );
    assert!(
        uses.iter().any(|use_| {
            use_.guards.iter().any(|guard| {
                matches!(
                    guard,
                    Guard::Not { path }
                    if path == "clickhouse.clickhouseOperator.serviceAccount.create"
                )
            })
        }),
        "expected a create=false branch for {path}; uses={uses:#?}"
    );
    let overlays = collection
        .contract_schema_signals()
        .conditional_path_overlays()
        .iter()
        .filter(|overlay| overlay.target_value_path == path)
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        overlays.iter().any(|overlay| {
            overlay.guards.iter().any(|guard| {
                matches!(
                    guard,
                    helm_schema_engine::ConditionalGuard::Truthy { path }
                    if path == "clickhouse.clickhouseOperator.serviceAccount.create"
                )
            })
        }),
        "expected a create=true conditional overlay for {path}; overlays={overlays:#?}"
    );
    assert!(
        overlays.iter().any(|overlay| {
            overlay.guards.iter().any(|guard| {
                matches!(
                    guard,
                    helm_schema_engine::ConditionalGuard::Not(inner)
                    if matches!(
                        inner.as_ref(),
                        helm_schema_engine::ConditionalGuard::Truthy { path }
                        if path == "clickhouse.clickhouseOperator.serviceAccount.create"
                    )
                )
            })
        }),
        "expected a create=false conditional overlay for {path}; overlays={overlays:#?}"
    );

    Ok(())
}

#[test]
fn signoz_root_service_account_name_keeps_helper_and_else_branch_guards()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("signoz-signoz");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let discovery = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&discovery.charts, false)?;
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(None),
    )?;
    let projection = collection.contract.clone().project();
    let path = "signoz.serviceAccount.name";
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        uses.iter().any(|use_| {
            use_.guards.iter().any(|guard| {
                matches!(
                    guard,
                    Guard::Truthy { path }
                    if path == "signoz.serviceAccount.create"
                )
            })
        }),
        "expected a create=true helper-backed branch for {path}; uses={uses:#?}"
    );
    assert!(
        uses.iter().any(|use_| {
            use_.guards.iter().any(|guard| {
                matches!(
                    guard,
                    Guard::Not { path }
                    if path == "signoz.serviceAccount.create"
                )
            })
        }),
        "expected a create=false helper-backed branch for {path}; uses={uses:#?}"
    );

    Ok(())
}

#[test]
fn signoz_otel_gateway_service_account_name_keeps_helper_default_nullability()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("signoz-signoz");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let discovery = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&discovery.charts, false)?;
    let values_yaml = chart::build_composed_values_yaml(&discovery.charts, true)?;
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(values_yaml.as_deref()),
    )?;
    let projection = collection.contract.clone().project();
    let path = "signoz-otel-gateway.serviceAccount.name";
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        uses.iter().any(|use_| {
            use_.guards.iter().any(|guard| {
                matches!(
                    guard,
                    Guard::Default { path }
                    if path == "signoz-otel-gateway.serviceAccount.name"
                )
            })
        }),
        "expected helper default guard for {path}; uses={uses:#?}"
    );
    assert!(
        collection
            .contract_schema_signals()
            .evidence_for(path)
            .is_some_and(|evidence| evidence.facts.is_nullable),
        "helper-defaulted subchart path should be globally nullable; facts={:#?}; uses={uses:#?}",
        collection
            .contract_schema_signals()
            .evidence_for(path)
            .map(|evidence| evidence.facts),
    );

    Ok(())
}

#[test]
fn signoz_clickhouse_security_context_records_fragment_fact() -> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("signoz-signoz");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let discovery = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&discovery.charts, false)?;
    let values_yaml = chart::build_composed_values_yaml(&discovery.charts, true)?;
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(values_yaml.as_deref()),
    )?;
    let projection = collection.contract.clone().project();
    let path = "clickhouse.securityContext";
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        collection
            .contract_schema_signals()
            .evidence_for(path)
            .is_some_and(|evidence| evidence.facts.used_as_fragment),
        "fragment-valued securityContext should not be pruned as a scalar parent; facts={:#?}; uses={uses:#?}",
        collection
            .contract_schema_signals()
            .evidence_for(path)
            .map(|evidence| evidence.facts),
    );

    Ok(())
}

#[test]
fn transitive_library_helper_default_flows_into_contract_requiredness_evidence()
-> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc::indoc! {"
            apiVersion: v2
            name: wrapper
            version: 0.1.0
            dependencies:
              - name: liba
                version: 0.1.0
              - name: libb
                version: 0.1.0
              - name: app
                version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "app: {}\n")?;

    test_util::write(
        &chart_dir.join("charts/liba/Chart.yaml")?,
        "apiVersion: v2\nname: liba\nversion: 0.1.0\ntype: library\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/liba/templates/_helpers.tpl")?,
        indoc::indoc! {r#"
            {{- define "liba.fullname" -}}
            {{- include "libb.name" . -}}
            {{- end -}}
        "#},
    )?;

    test_util::write(
        &chart_dir.join("charts/libb/Chart.yaml")?,
        "apiVersion: v2\nname: libb\nversion: 0.1.0\ntype: library\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/libb/templates/_helpers.tpl")?,
        indoc::indoc! {r#"
            {{- define "libb.name" -}}
            {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
            {{- end -}}
        "#},
    )?;

    test_util::write(
        &chart_dir.join("charts/app/Chart.yaml")?,
        "apiVersion: v2\nname: app\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/app/values.yaml")?,
        "nameOverride: ~\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/app/templates/cm.yaml")?,
        indoc::indoc! {r#"
            {{- if .Values.nameOverride }}
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: {{ include "liba.fullname" . }}
            {{- end }}
        "#},
    )?;

    let discovery = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&discovery.charts, false)?;
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(None),
    )?;
    let projection = collection.contract.clone().project();
    let name_override_uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == "app.nameOverride")
        .cloned()
        .collect::<Vec<_>>();

    let schema_signals = collection.contract_schema_signals();
    let evidence = schema_signals
        .evidence_for("app.nameOverride")
        .unwrap_or_else(|| {
            panic!("missing schema evidence for app.nameOverride; uses={name_override_uses:#?}")
        });
    assert!(
        evidence.requiredness.has_default_fallback,
        "transitive library helper default should become path-local contract evidence, got evidence={evidence:#?}; uses={name_override_uses:#?}",
    );

    Ok(())
}

#[test]
fn cert_manager_fullname_override_records_self_guarded_render_evidence()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("cert-manager");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let discovery = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&discovery.charts, false)?;
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(None),
    )?;
    let path = "fullnameOverride";
    let projection = collection.contract.clone().project();
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();
    let schema_signals = collection.contract_schema_signals();
    let facts = schema_signals
        .evidence_for(path)
        .map(|evidence| evidence.facts)
        .unwrap_or_else(|| panic!("missing facts for {path}; uses={uses:#?}"));

    assert!(
        facts.has_self_guarded_render_use,
        "helper override path should carry at least one self-guarded render use; facts={facts:#?}; uses={uses:#?}"
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
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(None),
    )?;
    let projection = collection.contract.project();
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| {
            use_.source_expr == "kid.enabled"
                && use_.path.0 == ["data".to_string(), "enabled".to_string()]
        })
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        uses.iter().any(|use_| use_.guards.as_slice()
            == [Guard::Truthy {
                path: "kid.enabled".to_string()
            }]
            .as_slice()),
        "expected first condition activation branch, got {uses:#?}"
    );
    assert!(
        uses.iter().any(|use_| use_.guards.as_slice()
            == [
                Guard::Absent {
                    path: "kid.enabled".to_string()
                },
                Guard::Truthy {
                    path: "global.kidEnabled".to_string()
                },
            ]
            .as_slice()),
        "expected second condition branch guarded by first-condition absence, got {uses:#?}"
    );
    assert!(
        uses.iter().any(|use_| use_.guards.as_slice()
            == [
                Guard::Absent {
                    path: "kid.enabled".to_string()
                },
                Guard::Absent {
                    path: "global.kidEnabled".to_string()
                },
                Guard::Truthy {
                    path: "tags.observability".to_string()
                },
            ]
            .as_slice()),
        "expected tag activation branch guarded by condition absence, got {uses:#?}"
    );
    assert!(
        uses.iter().any(|use_| use_.guards.as_slice()
            == [
                Guard::Absent {
                    path: "kid.enabled".to_string()
                },
                Guard::Absent {
                    path: "global.kidEnabled".to_string()
                },
                Guard::Absent {
                    path: "tags.observability".to_string()
                },
            ]
            .as_slice()),
        "expected default-active branch when no activation values exist, got {uses:#?}"
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
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(None),
    )?;
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
    let collection = analyze_charts(
        &discovery.charts,
        &defines,
        false,
        &crate::values_roots::top_level_value_paths(None),
    )?;
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
