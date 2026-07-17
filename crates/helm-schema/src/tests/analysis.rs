use helm_schema_ir::{Guard, ResourceRef, YamlPath};
use helm_schema_k8s::{ChartLocalCrdSchemaProvider, Diagnostic, DiagnosticSink, K8sSchemaProvider};
use serde_json::json;
use vfs::VfsPath;

use crate::analysis::analyze_charts;
use test_util::prelude::sim_assert_eq;

use crate::chart;

macro_rules! contract_schema_signals {
    ($collection:expr) => {
        $collection
            .contract
            .clone()
            .finalize()
            .into_schema_signals()
    };
}

#[test]
fn one_variable_integer_range_emits_input_channel_diagnostic() {
    let defines = helm_schema_ast::DefineIndex::new();
    let signals = helm_schema_ir::SymbolicIrContext::new(&defines)
        .generate_contract_ir(
            r#"{{- range .Values.servers }}
{{ . | quote }}
{{- end }}"#,
        )
        .finalize()
        .into_schema_signals();
    let diagnostics = DiagnosticSink::new();

    crate::session::emit_input_channel_diagnostics(&signals, &diagnostics);

    sim_assert_eq!(
        have: diagnostics.snapshot(),
        want: vec![Diagnostic::InputChannelNumericRangeAmbiguity {
            value_path: "servers".to_string(),
        }]
    );
}

#[test]
fn two_variable_range_has_no_numeric_input_channel_ambiguity() {
    let defines = helm_schema_ast::DefineIndex::new();
    let signals = helm_schema_ir::SymbolicIrContext::new(&defines)
        .generate_contract_ir(
            r#"{{- range $key, $value := .Values.servers }}
{{ $key }}={{ $value }}
{{- end }}"#,
        )
        .finalize()
        .into_schema_signals();
    let diagnostics = DiagnosticSink::new();

    crate::session::emit_input_channel_diagnostics(&signals, &diagnostics);

    sim_assert_eq!(have: diagnostics.snapshot(), want: Vec::<Diagnostic>::new());
}

#[test]
fn airflow_break_scopes_the_deprecated_security_context_candidate() -> color_eyre::eyre::Result<()>
{
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("airflow");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let signals = contract_schema_signals!(collection);
    let evidence = signals
        .evidence_for("workers.securityContext")
        .expect("workers.securityContext evidence");

    assert!(
        evidence.provider_schema_uses.is_empty()
            && evidence
                .conditional_overlays
                .iter()
                .any(|overlay| { !overlay.evidence.provider_schema_uses.is_empty() }),
        "the later candidate must only carry provider evidence behind its no-prior-break guard: {evidence:#?}"
    );

    Ok(())
}

#[test]
fn loki_selected_htpasswd_default_program_reaches_required_credentials()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata().join("charts").join("loki");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let values_yaml = chart::build_composed_values_yaml(&charts, true)?;
    let values_roots = crate::values_roots::ValuesRoots::from_values_yaml(values_yaml.as_deref());
    assert!(
        values_roots
            .string_defaults
            .contains_key("gateway.basicAuth.htpasswd"),
        "the composed values document must preserve the chart-authored program"
    );
    let collection = analyze_charts(&charts, &defines, false, &values_roots)?;
    let signals = contract_schema_signals!(collection);

    for path in ["gateway.basicAuth.username", "gateway.basicAuth.password"] {
        assert!(
            signals.terminal_clauses().iter().any(|clause| clause
                .iter()
                .flat_map(helm_schema_core::ConditionalGuard::value_paths)
                .any(|guard_path| guard_path == path)),
            "the selected htpasswd default must retain its required call for {path}: {signals:#?}"
        );
    }

    Ok(())
}

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

    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let path = "kid.controller.ingressClassResource.parameters";

    let ir_fact = contract_schema_signals!(collection)
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
fn signoz_zookeeper_name_override_string_contract_stays_branch_scoped()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("signoz-signoz");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let path = "clickhouse.zookeeper.nameOverride";

    // The zookeeper naming helpers consume the raw operand of
    // `default .Chart.Name .Values.nameOverride`: `common.names.fullname`
    // runs `contains $name .Release.Name` (Sprig `contains` aborts on
    // non-strings — helm rejects an integer nameOverride with "wrong type
    // for value; expected string") and `common.names.name` pipes it into
    // `trunc`. The string implications must exist, and every one must ride
    // the operand's own truthiness: a falsy nameOverride selects the chart
    // name and renders, so the falsy set stays open. (Kind-payload
    // captures crossing the subchart boundary once lost their path prefix,
    // which hid these implications from this parent-scoped path entirely.)
    let schema_signals = contract_schema_signals!(collection);
    let evidence = schema_signals
        .evidence_for(path)
        .unwrap_or_else(|| panic!("missing evidence for {path}"));
    let string_implications: Vec<_> =
        evidence
            .fail_implications
            .iter()
            .filter(|implication| {
                implication.requirements.contains(
                    &helm_schema_core::FailValueRequirement::SchemaType("string".to_string()),
                )
            })
            .collect();
    assert!(
        !string_implications.is_empty(),
        "the contains/trunc consumers type {path} as string; evidence={evidence:?}"
    );
    assert!(
        string_implications.iter().all(|implication| {
            implication.outer_guards.iter().any(|guard| {
                matches!(
                    guard,
                    helm_schema_core::ConditionalGuard::Truthy { path: guard_path }
                        if guard_path == path
                )
            })
        }),
        "every string implication rides the operand's own truthiness; \
         implications={string_implications:?}"
    );

    Ok(())
}

#[test]
fn signoz_clickhouse_operator_image_helper_printf_binds_no_string_contract()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("signoz-signoz");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let path = "clickhouse.clickhouseOperator.image.repository";

    // The operator image helper renders the repository only through printf,
    // which stringifies any argument: the evidence must exist for the
    // scoped path but must not carry a string input contract.
    assert!(
        contract_schema_signals!(collection)
            .evidence_for(path)
            .is_some_and(|evidence| !evidence.type_hints.contains("string")),
        "printf must not bind a string contract on {path}; contract_hints={:?}",
        contract_schema_signals!(collection).schema_evidence_by_value_path(),
    );

    Ok(())
}

#[test]
fn promtail_helper_string_consumer_reaches_the_image_tag_contract() -> color_eyre::eyre::Result<()>
{
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("promtail");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let path = "image.tag";
    let signals = contract_schema_signals!(collection);
    let evidence = signals.evidence_for(path);

    assert!(
        evidence.is_some_and(|evidence| {
            evidence.type_hints.contains("string")
                || evidence.fail_implications.iter().any(|implication| {
                    implication.requirements.contains(
                        &helm_schema_core::FailValueRequirement::SchemaType("string".to_string()),
                    )
                })
        }),
        "the helper's regex replacement must retain its string subject contract: {evidence:#?}"
    );

    Ok(())
}

#[test]
fn signoz_smtp_existing_secret_name_is_rendered_as_secret_ref_name() -> color_eyre::eyre::Result<()>
{
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("signoz-signoz");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let projection = collection.contract.clone().finalize();
    let path = "signoz.smtpVars.existingSecret.name";
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        uses.iter().any(|use_| {
            use_.path.0
                == [
                    "spec".to_string(),
                    "template".to_string(),
                    "spec".to_string(),
                    "containers[*]".to_string(),
                    "env[*]".to_string(),
                    "valueFrom".to_string(),
                    "secretKeyRef".to_string(),
                    "name".to_string(),
                ]
                && use_.resource.as_ref().is_some_and(|resource| {
                    resource.api_version == "apps/v1" && resource.kind == "StatefulSet"
                })
        }),
        "expected render use for {path} at secretKeyRef.name; uses={uses:#?}",
    );
    let signals = contract_schema_signals!(collection);
    let evidence = signals
        .evidence_for(path)
        .unwrap_or_else(|| panic!("missing schema evidence for {path}; uses={uses:#?}"));
    assert!(
        evidence.is_referenced_value_path,
        "expected {path} to be a referenced value path; evidence={evidence:#?}; uses={uses:#?}",
    );
    assert!(
        evidence.provider_schema_uses.is_empty(),
        "guarded provider evidence must not constrain the base path; evidence={evidence:#?}; uses={uses:#?}",
    );
    let overlay = evidence.conditional_overlays.first().unwrap_or_else(|| {
        panic!("missing guarded provider overlay for {path}; evidence={evidence:#?}")
    });
    sim_assert_eq!(
        have: &overlay.guards,
        want: &vec![
            helm_schema_ir::ConditionalGuard::Truthy {
                path: "signoz.smtpVars.enabled".to_string(),
            },
            helm_schema_ir::ConditionalGuard::AnyOf(vec![
                helm_schema_ir::ConditionalGuard::Truthy {
                    path: "signoz.smtpVars.existingSecret.fromKey".to_string(),
                },
                helm_schema_ir::ConditionalGuard::Truthy {
                    path: "signoz.smtpVars.existingSecret.hostKey".to_string(),
                },
                helm_schema_ir::ConditionalGuard::Truthy {
                    path: "signoz.smtpVars.existingSecret.passwordKey".to_string(),
                },
                helm_schema_ir::ConditionalGuard::Truthy {
                    path: "signoz.smtpVars.existingSecret.portKey".to_string(),
                },
                helm_schema_ir::ConditionalGuard::Truthy {
                    path: "signoz.smtpVars.existingSecret.usernameKey".to_string(),
                },
            ]),
        ]
    );
    let provider_use = overlay
        .evidence
        .provider_schema_uses
        .first()
        .unwrap_or_else(|| {
            panic!("missing branch-local provider use for {path}; overlay={overlay:#?}")
        });
    sim_assert_eq!(
        have: &provider_use.path.0,
        want: &vec![
            "spec".to_string(),
            "template".to_string(),
            "spec".to_string(),
            "containers[*]".to_string(),
            "env[*]".to_string(),
            "valueFrom".to_string(),
            "secretKeyRef".to_string(),
            "name".to_string(),
        ]
    );
    sim_assert_eq!(have: provider_use.resource.api_version.as_str(), want: "apps/v1");
    sim_assert_eq!(have: provider_use.resource.kind.as_str(), want: "StatefulSet");

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
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let projection = collection.contract.clone().finalize();
    let path = "clickhouse.clickhouseOperator.serviceAccount.name";
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        uses.iter().any(|use_| {
            use_.condition
                .guard_conjunctions()
                .iter()
                .flatten()
                .any(|guard| {
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
            use_.condition
                .guard_conjunctions()
                .iter()
                .flatten()
                .any(|guard| {
                    matches!(
                        guard,
                        Guard::Not { path }
                        if path == "clickhouse.clickhouseOperator.serviceAccount.create"
                    )
                })
        }),
        "expected a create=false branch for {path}; uses={uses:#?}"
    );
    let overlays = contract_schema_signals!(collection)
        .evidence_for(path)
        .map(|evidence| evidence.conditional_overlays.clone())
        .unwrap_or_default();
    assert!(
        overlays.iter().any(|overlay| {
            overlay.guards.iter().any(|guard| {
                matches!(
                    guard,
                    helm_schema_ir::ConditionalGuard::Truthy { path }
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
                    helm_schema_ir::ConditionalGuard::Not(inner)
                    if matches!(
                        inner.as_ref(),
                        helm_schema_ir::ConditionalGuard::Truthy { path }
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
fn traefik_host_users_keeps_provider_sink_under_invalid_kind_guard() -> color_eyre::eyre::Result<()>
{
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("traefik");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let path = "deployment.hostUsers";
    let signals = contract_schema_signals!(collection);
    let evidence = signals
        .evidence_for(path)
        .unwrap_or_else(|| panic!("missing schema evidence for {path}"));

    assert!(
        !evidence.provider_schema_uses.is_empty()
            || evidence
                .conditional_overlays
                .iter()
                .any(|overlay| !overlay.evidence.provider_schema_uses.is_empty()),
        "expected provider sink evidence for {path}; evidence={evidence:#?}",
    );

    Ok(())
}

#[test]
fn prometheus_namespace_helper_keeps_join_conversion_boundary() -> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("prometheus");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let path = "server.namespaces";
    let signals = contract_schema_signals!(collection);
    let evidence = signals
        .evidence_for(path)
        .unwrap_or_else(|| panic!("missing schema evidence for {path}"));

    assert!(
        evidence.facts.used_as_serialized,
        "join inside the namespace helper must keep its conversion boundary; evidence={evidence:#?}",
    );

    Ok(())
}

#[test]
fn signoz_root_service_account_name_keeps_resource_scope_and_default_semantics()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("signoz-signoz");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let projection = collection.contract.clone().finalize();
    let path = "signoz.serviceAccount.name";
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();

    sim_assert_eq!(have: uses.len(), want: 2, "uses={uses:#?}");
    let service_account_use = uses
        .iter()
        .find(|use_| {
            use_.resource.as_ref().is_some_and(|resource| {
                resource.api_version == "v1" && resource.kind == "ServiceAccount"
            })
        })
        .unwrap_or_else(|| {
            panic!("missing ServiceAccount metadata.name use for {path}; uses={uses:#?}")
        });
    assert!(
        service_account_use
            .single_guard_conjunction()
            .iter()
            .any(|guard| matches!(guard, Guard::Truthy { path } if path == "signoz.serviceAccount.create")),
        "the conditional ServiceAccount resource must retain its create guard; use={service_account_use:#?}"
    );
    let stateful_set_use = uses
        .iter()
        .find(|use_| {
            use_.resource.as_ref().is_some_and(|resource| {
                resource.api_version == "apps/v1" && resource.kind == "StatefulSet"
            })
        })
        .unwrap_or_else(|| {
            panic!("missing StatefulSet serviceAccountName use for {path}; uses={uses:#?}")
        });
    let stateful_set_guards = stateful_set_use.single_guard_conjunction();
    assert!(
        stateful_set_guards.iter().any(
            |guard| matches!(guard, Guard::Default { path } if path == "signoz.serviceAccount.name")
        ),
        "both helper arms default the service account name; use={stateful_set_use:#?}"
    );
    assert!(
        stateful_set_guards.iter().all(|guard| !matches!(
            guard,
            Guard::Truthy { path } | Guard::Not { path }
            if path == "signoz.serviceAccount.create"
        )),
        "the unconditional StatefulSet consumes the same value in both helper arms; use={stateful_set_use:#?}"
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
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let values_yaml = chart::build_composed_values_yaml(&charts, true)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(values_yaml.as_deref()),
    )?;
    let projection = collection.contract.clone().finalize();
    let path = "signoz-otel-gateway.serviceAccount.name";
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        uses.iter().any(|use_| {
            use_.condition
                .guard_conjunctions()
                .iter()
                .flatten()
                .any(|guard| {
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
        contract_schema_signals!(collection)
            .evidence_for(path)
            .is_some_and(|evidence| evidence.facts.is_nullable),
        "helper-defaulted subchart path should be globally nullable; facts={:#?}; uses={uses:#?}",
        contract_schema_signals!(collection)
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
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let values_yaml = chart::build_composed_values_yaml(&charts, true)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(values_yaml.as_deref()),
    )?;
    let projection = collection.contract.clone().finalize();
    let path = "clickhouse.securityContext";
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        contract_schema_signals!(collection)
            .evidence_for(path)
            .is_some_and(|evidence| evidence.facts.used_as_fragment),
        "fragment-valued securityContext should not be pruned as a scalar parent; facts={:#?}; uses={uses:#?}",
        contract_schema_signals!(collection)
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

    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let projection = collection.contract.clone().finalize();
    let name_override_uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == "app.nameOverride")
        .cloned()
        .collect::<Vec<_>>();

    let schema_signals = contract_schema_signals!(collection);
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
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let path = "fullnameOverride";
    let projection = collection.contract.clone().finalize();
    let uses = projection
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == path)
        .cloned()
        .collect::<Vec<_>>();
    let schema_signals = contract_schema_signals!(collection);
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
fn cert_manager_webhook_values_root_is_seeded_without_dependency_fragment()
-> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join("cert-manager");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let values_yaml = chart::build_composed_values_yaml(&charts, true)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(values_yaml.as_deref()),
    )?;
    let path = "webhook";
    let signals = contract_schema_signals!(collection);
    let evidence = signals
        .evidence_for(path)
        .unwrap_or_else(|| panic!("missing values-root evidence for {path}"));

    assert!(
        !evidence.facts.used_as_pathless_fragment,
        "local values root should not be seeded as a pathless fragment; evidence={evidence:#?}",
    );
    assert!(
        !evidence.facts.accepted_dependency_values_root_fragment,
        "local mapping-backed values root should not be treated as a dependency-root fragment; evidence={evidence:#?}",
    );
    assert!(
        evidence.is_referenced_value_path,
        "values root should still be resolved as an accepted values path; evidence={evidence:#?}",
    );
    assert!(
        evidence.conditional_overlays.is_empty(),
        "values root seed should not gain conditional overlays; evidence={evidence:#?}",
    );

    let opts = crate::GenerateOptions {
        chart_dir: chart_dir.clone(),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        provider: crate::provider::ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(
                test_util::workspace_root().join(".cache/kubernetes-json-schema-cache"),
            ),
            allow_net: false,
            disable_k8s_schemas: false,
            crd_override_dir: Some(test_util::workspace_root().join(".cache/crds-catalog-cache")),
            ..Default::default()
        },
    };
    let session = crate::AnalysisSession::new(opts);
    let session_signals = session.contract_schema_signals()?;
    let session_evidence = session_signals
        .evidence_for(path)
        .unwrap_or_else(|| panic!("missing session schema evidence for {path}"));
    assert!(
        session_evidence.is_referenced_value_path
            && !session_evidence.facts.used_as_pathless_fragment
            && !session_evidence
                .facts
                .accepted_dependency_values_root_fragment,
        "session should preserve local root evidence without dependency-fragment widening; evidence={session_evidence:#?}",
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

    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let projection = collection.contract.finalize();
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
        uses.iter().any(|use_| {
            use_.condition.guard_conjunctions().iter().any(|guards| {
                guards.as_slice()
                    == [Guard::Truthy {
                        path: "kid.enabled".to_string(),
                    }]
                    .as_slice()
            })
        }),
        "expected first condition activation branch, got {uses:#?}"
    );
    assert!(
        uses.iter().any(|use_| {
            use_.condition.guard_conjunctions().iter().any(|guards| {
                guards.as_slice()
                    == [
                        Guard::Truthy {
                            path: "global.kidEnabled".to_string(),
                        },
                        Guard::Absent {
                            path: "kid.enabled".to_string(),
                        },
                    ]
                    .as_slice()
            })
        }),
        "expected second condition branch guarded by first-condition absence, got {uses:#?}"
    );
    assert!(
        uses.iter().any(|use_| {
            use_.condition.guard_conjunctions().iter().any(|guards| {
                guards.as_slice()
                    == [
                        Guard::Truthy {
                            path: "tags.observability".to_string(),
                        },
                        Guard::Absent {
                            path: "global.kidEnabled".to_string(),
                        },
                        Guard::Absent {
                            path: "kid.enabled".to_string(),
                        },
                    ]
                    .as_slice()
            })
        }),
        "expected tag activation branch guarded by condition absence, got {uses:#?}"
    );
    assert!(
        uses.iter().any(|use_| {
            use_.condition.guard_conjunctions().iter().any(|guards| {
                guards.as_slice()
                    == [
                        Guard::Absent {
                            path: "global.kidEnabled".to_string(),
                        },
                        Guard::Absent {
                            path: "kid.enabled".to_string(),
                        },
                        Guard::Absent {
                            path: "tags.observability".to_string(),
                        },
                    ]
                    .as_slice()
            })
        }),
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

    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let provider = ChartLocalCrdSchemaProvider::new(collection.local_schema_universe);
    let resource = ResourceRef::concrete("example.com/v1".to_string(), "Widget".to_string());

    let schema = provider
        .lookup(
            &resource,
            &YamlPath(vec!["spec".to_string(), "size".to_string()]),
        )
        .into_schema_fragment();

    sim_assert_eq!(
        have: schema.map(|fragment| fragment.into_schema()),
        want: Some(json!({"type": "integer"}))
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

    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;
    let provider = ChartLocalCrdSchemaProvider::new(collection.local_schema_universe);
    let resource = ResourceRef::concrete("example.com/v1".to_string(), "Widget".to_string());

    let schema = provider
        .lookup(
            &resource,
            &YamlPath(vec!["spec".to_string(), "size".to_string()]),
        )
        .into_schema_fragment();

    sim_assert_eq!(
        have: schema.map(|fragment| fragment.into_schema()),
        want: Some(json!({"type": "integer"}))
    );

    Ok(())
}

/// A bitnami-style `validateValues` aggregator joins conditionally-produced
/// messages and fails on the joined text. The joined value's truthiness is
/// NOT the collection's non-emptiness, so the `if $message` guard must not
/// vanish — with a dependency activation condition appended, a vanished
/// guard becomes a chart-killing `activation => fail` terminal clause.
#[test]
fn joined_validator_messages_do_not_become_activation_terminals() -> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\ndependencies:\n  - name: child\n    version: 0.1.0\n    condition: child.enabled\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "child:\n  enabled: true\n")?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "apiVersion: v2\nname: child\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        "enabled: true\nauth:\n  enabled: false\n  user: \"\"\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/_helpers.tpl")?,
        r#"{{- define "child.validateValues.auth" -}}
{{- if and .Values.auth.enabled (not .Values.auth.user) }}
child: auth.enabled
    In order to enable authentication, you need to provide a user.
{{- end -}}
{{- end -}}

{{- define "child.validateValues" -}}
{{- $messages := list -}}
{{- $messages := append $messages (include "child.validateValues.auth" .) -}}
{{- $messages := without $messages "" -}}
{{- $message := join "\n" $messages -}}

{{- if $message -}}
{{-   printf "\nVALUES VALIDATION:\n%s" $message | fail -}}
{{- end -}}
{{- end -}}
"#,
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/NOTES.txt")?,
        "Thank you for installing.\n{{- include \"child.validateValues\" . }}\n",
    )?;

    let charts = chart::discover_chart_contexts(&chart_dir)?;
    let defines = chart::build_define_index(&charts, false)?;
    let collection = analyze_charts(
        &charts,
        &defines,
        false,
        &crate::values_roots::ValuesRoots::from_values_yaml(None),
    )?;

    let terminal_clauses = contract_schema_signals!(collection)
        .terminal_clauses()
        .to_vec();
    assert!(
        terminal_clauses.is_empty(),
        "the conditional validator must not terminate the activated chart: {terminal_clauses:#?}"
    );

    Ok(())
}

#[test]
fn tpl_executes_only_the_selected_chart_authored_default_program() -> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    let program = r#"{{ required "username required" .Values.auth.username }}:{{ required "password required" .Values.auth.password }}"#;

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        format!(
            "auth:\n  enabled: true\n  username: \"\"\n  password: \"\"\n  program: |-\n    {}\n",
            program
        ),
    )?;
    test_util::write(
        &chart_dir.join("templates/secret.yaml")?,
        r#"apiVersion: v1
kind: Secret
metadata:
  name: test
{{- with .Values.auth }}
{{- if .enabled }}
stringData:
  credentials: |
    {{- tpl .program $ | nindent 4 }}
{{- end }}
{{- end }}
"#,
    )?;

    let schema = crate::AnalysisSession::new(crate::GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        provider: crate::provider::ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            ..Default::default()
        },
    })
    .generated_schema()?
    .schema;
    let validator = jsonschema::validator_for(&schema)?;
    let default_auth = json!({
        "username": "",
        "password": "",
        "program": program
    });

    assert!(
        !validator.is_valid(&json!({ "auth": default_auth })),
        "the selected default program executes both required calls: {schema}"
    );
    assert!(validator.is_valid(&json!({
        "auth": {
            "username": "user",
            "password": "pass",
            "program": program
        }
    })));
    assert!(
        validator.is_valid(&json!({
            "auth": { "username": "", "password": "", "program": "fixed" }
        })),
        "an override replaces the chart-authored program and its requirements: {schema}"
    );
    assert!(
        !validator.is_valid(&json!({
            "auth": { "username": "", "password": "", "program": program }
        })),
        "explicitly selecting the same program keeps its requirements: {schema}"
    );

    Ok(())
}
