use super::*;
use crate::{
    CompletionPass, SchemaProfile,
    emission_plan::LoweredEmissionPlan,
    emission_policy::{EmissionClassKind, EmissionPolicy},
    generate_values_schema_with_report,
};
use color_eyre::eyre;
use std::sync::atomic::{AtomicUsize, Ordering};
use test_util::prelude::sim_assert_eq;

#[test]
fn lean_profile_keeps_local_conditionals_and_omits_document_level_conditionals() {
    let source = indoc! {r#"
        {{- if .Values.enabled }}
        data:
          message: {{ .Values.message | quote }}
        {{- end }}
        {{- if .Values.forbidden }}
        {{- fail "forbidden" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
    "#};
    let values_yaml = indoc! {"
        enabled: false
        forbidden: false
        message: hello
    "};
    let signals = schema_signals_for(parse_ir(source));

    let default_schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider).with_values_yaml(Some(values_yaml)),
    );
    let full_schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_yaml(Some(values_yaml))
            .with_profile(SchemaProfile::Full),
    );
    let lean_schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_yaml(Some(values_yaml))
            .with_profile(SchemaProfile::Lean),
    );

    sim_assert_eq!(have: default_schema, want: full_schema);
    sim_assert_eq!(
        have: lean_schema,
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "enabled": {},
                "forbidden": {},
                "message": {},
            },
            "type": "object",
        })
    );

    for instance in [
        serde_json::json!({}),
        serde_json::json!({
            "enabled": false,
            "forbidden": false,
            "message": "hello",
        }),
        serde_json::json!({
            "enabled": true,
            "forbidden": false,
            "message": "hello",
        }),
    ] {
        assert!(schema_accepts_instance(&full_schema, &instance));
        assert!(schema_accepts_instance(&lean_schema, &instance));
    }
    assert!(!schema_accepts_instance(
        &full_schema,
        &serde_json::json!({ "forbidden": true })
    ));
    assert!(schema_accepts_instance(
        &lean_schema,
        &serde_json::json!({ "forbidden": true })
    ));
}

#[test]
fn emission_report_conserves_facts_and_keeps_mandatory_facts() {
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          member: {{ .Values.host.member }}
        {{- if .Values.enabled }}
          guarded: {{ .Values.guarded }}
        {{- end }}
        {{- if .Values.forbidden }}
        {{- fail "forbidden" }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: false
        forbidden: false
        guarded: value
        host:
          member: value
    "};
    let signals = schema_signals_for(parse_ir(source));
    let (_, full) = generate_values_schema_with_report(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_yaml(Some(values_yaml))
            .with_profile(SchemaProfile::Full),
    );
    let (_, lean) = generate_values_schema_with_report(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_yaml(Some(values_yaml))
            .with_profile(SchemaProfile::Lean),
    );

    for report in [&full, &lean] {
        sim_assert_eq!(
            have: report.facts.lowered,
            want: report.facts.selected + report.facts.dropped
        );
    }
    let full_mandatory =
        full.counts_for_class(crate::emission_policy::EmissionClassKind::Mandatory);
    assert!(full_mandatory.lowered > 0);
    sim_assert_eq!(have: full_mandatory.dropped, want: 0);
    sim_assert_eq!(
        have: full_mandatory.selected,
        want: full.mandatory_outcomes.total()
    );
    let lean_mandatory =
        lean.counts_for_class(crate::emission_policy::EmissionClassKind::Mandatory);
    sim_assert_eq!(have: lean_mandatory.dropped, want: 0);
    sim_assert_eq!(
        have: lean_mandatory.selected,
        want: lean.mandatory_outcomes.total()
    );
    assert!(lean_mandatory.selected > 0);
    assert!(full.facts.selected > lean.facts.selected);
    for report in [&full, &lean] {
        sim_assert_eq!(
            have: report.insertion_abstentions,
            want: crate::InsertionAbstentionCounts::default()
        );
    }
    for class in [
        crate::emission_policy::EmissionClassKind::OrdinaryRoot,
        crate::emission_policy::EmissionClassKind::KindPartitionRoot,
        crate::emission_policy::EmissionClassKind::KindPartitionLocal,
        crate::emission_policy::EmissionClassKind::TerminalAlways,
        crate::emission_policy::EmissionClassKind::TerminalGuarded,
    ] {
        sim_assert_eq!(have: lean.counts_for_class(class).selected, want: 0);
    }
    let lean_local =
        lean.counts_for_class(crate::emission_policy::EmissionClassKind::OrdinaryLocal);
    sim_assert_eq!(have: lean_local.selected, want: lean_local.lowered);
}

#[test]
fn member_projection_reports_ambiguous_descendant_insertion() {
    use crate::overlay_lowering::member_descendant_projection;
    use crate::path_resolver::ResolvedPathSchema;

    let root = ResolvedPathSchema {
        value_path: "items.*".to_string(),
        path_segments: vec!["items".to_string(), "*".to_string()],
        schema: serde_json::json!({}),
        structural_schema: serde_json::json!({
            "anyOf": [
                {
                    "properties": { "member": { "type": "integer" } },
                    "required": ["member"],
                    "type": "object",
                },
                {
                    "properties": { "member": { "type": "boolean" } },
                    "required": ["member"],
                    "type": "object",
                },
            ],
        }),
        values_yaml_schema: serde_json::json!({}),
        provider_schema_candidate: None,
        used_as_serialized: false,
        used_as_pathless_fragment: false,
        accepted_dependency_values_root_fragment: false,
    };
    let descendant = ResolvedPathSchema {
        value_path: "items.*.member".to_string(),
        path_segments: vec!["items".to_string(), "*".to_string(), "member".to_string()],
        schema: serde_json::json!({ "type": "string" }),
        structural_schema: serde_json::json!({ "type": "string" }),
        values_yaml_schema: serde_json::json!({}),
        provider_schema_candidate: None,
        used_as_serialized: false,
        used_as_pathless_fragment: false,
        accepted_dependency_values_root_fragment: false,
    };
    let mut abstentions = 0;

    let projected = member_descendant_projection(
        &["items".to_string()],
        &[&root, &descendant],
        &mut abstentions,
    );

    sim_assert_eq!(have: abstentions, want: 1);
    sim_assert_eq!(have: projected, want: Some(root.structural_schema));
}

#[test]
fn kind_partition_policy_requires_at_least_one_anchor_lane() {
    use crate::emission_policy::{ConditionalAnchors, EmissionPolicy};

    assert!(EmissionPolicy::new(ConditionalAnchors::Local, true, true).is_ok());
    assert!(EmissionPolicy::new(ConditionalAnchors::Root, true, true).is_ok());
    assert!(EmissionPolicy::new(ConditionalAnchors::None, true, true).is_err());
    assert!(EmissionPolicy::new(ConditionalAnchors::None, true, false).is_ok());
}

#[test]
fn kind_partition_audit_retains_local_anchors() {
    let source = indoc! {r#"
        apiVersion: apps/v1
        kind: {{ .Values.workload.kind }}
        metadata:
          name: test
        spec:
          {{- if eq .Values.workload.kind "Deployment" }}
          strategy: {{- toYaml .Values.workload.strategy | nindent 4 }}
          {{- else if eq .Values.workload.kind "StatefulSet" }}
          updateStrategy: {{- toYaml .Values.workload.strategy | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        workload:
          kind: Deployment
          strategy:
            type: RollingUpdate
    "};
    let signals = schema_signals_for(parse_ir(source));
    let (_, report) = generate_values_schema_with_report(
        ValuesSchemaInput::new(&signals, &provider()).with_values_yaml(Some(values_yaml)),
    );
    let local_partitions =
        report.counts_for_class(crate::emission_policy::EmissionClassKind::KindPartitionLocal);

    assert!(local_partitions.lowered > 0);
    sim_assert_eq!(have: local_partitions.selected, want: local_partitions.lowered);
}

#[test]
fn completion_passes_preserve_profile_monotonicity() -> eyre::Result<()> {
    let helpers = indoc! {r#"
        {{- define "test.tplValues" -}}
        {{- $doc := .doc -}}
        {{- if and (eq (kindOf $doc) "map") (eq (len $doc) 1) (hasKey $doc "$tplYaml") -}}
        {{- $tpl := get $doc "$tplYaml" -}}
        {{- toJson (dict "doc" (fromYaml (tpl $tpl .ctx))) -}}
        {{- else -}}
        {{- toJson (dict "doc" $doc) -}}
        {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        {{- $values := get (include "test.tplValues" (dict "doc" .Values "ctx" $) | fromJson) "doc" }}
        {{- $_ := set . "Values" $values }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- if eq .Values.mode "on" }}
        data:
          result: {{ add .Values.payload 1 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        mode: off
        payload: 1
    "};
    let signals = schema_signals_for(parse_ir_with_helpers(source, helpers));
    let input = ValuesSchemaInput::new(&signals, &NoopProvider).with_values_yaml(Some(values_yaml));
    let plan = LoweredEmissionPlan::build(&input);
    let full_policy = EmissionPolicy::for_profile(SchemaProfile::Full);
    let passes = [
        CompletionPass::Projected,
        CompletionPass::ValuesDefaultBackfill,
        CompletionPass::OpenGlobal,
        CompletionPass::DeclaredDefaults,
        CompletionPass::RepeatedProviderPayloads,
        CompletionPass::SharedDefinitions,
        CompletionPass::ProgramWrappers,
        CompletionPass::Descriptions,
    ];
    let modes = [
        serde_json::json!(null),
        serde_json::json!("off"),
        serde_json::json!("on"),
    ];
    let payloads = [
        serde_json::json!(null),
        serde_json::json!(0),
        serde_json::json!("1"),
        serde_json::json!("wrong"),
        serde_json::json!({}),
        serde_json::json!({ "$tplYaml": "2" }),
    ];

    for pass in passes {
        let full = plan.complete(plan.project(full_policy), pass).schema;
        let lean = plan
            .complete(
                plan.project(EmissionPolicy::for_profile(SchemaProfile::Lean)),
                pass,
            )
            .schema;
        let full = jsonschema::validator_for(&full)
            .map_err(|error| eyre::eyre!("compile full schema after {pass:?}: {error}"))?;
        let lean = jsonschema::validator_for(&lean)
            .map_err(|error| eyre::eyre!("compile lean schema after {pass:?}: {error}"))?;
        for mode in &modes {
            for payload in &payloads {
                let instance = serde_json::json!({ "mode": mode, "payload": payload });
                eyre::ensure!(
                    !full.is_valid(&instance) || lean.is_valid(&instance),
                    "{pass:?} narrowed the widened profile for {instance}"
                );
            }
        }
    }

    Ok(())
}

#[test]
fn one_plan_projections_obey_floors_and_ignore_projection_order() {
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          member: {{ .Values.host.member }}
        {{- if .Values.enabled }}
          guarded: {{ .Values.guarded }}
        {{- end }}
        {{- if .Values.forbidden }}
        {{- fail "forbidden" }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: false
        forbidden: false
        guarded: value
        host:
          member: value
    "};
    let signals = schema_signals_for(parse_ir(source));
    let input = ValuesSchemaInput::new(&signals, &NoopProvider).with_values_yaml(Some(values_yaml));
    let plan = LoweredEmissionPlan::build(&input);
    let full_policy = EmissionPolicy::for_profile(SchemaProfile::Full);
    let lean_policy = EmissionPolicy::for_profile(SchemaProfile::Lean);

    let full_first = plan.complete(plan.project(full_policy), CompletionPass::Descriptions);
    let lean_second = plan.complete(plan.project(lean_policy), CompletionPass::Descriptions);
    let lean_first = plan.complete(plan.project(lean_policy), CompletionPass::Descriptions);
    let full_second = plan.complete(plan.project(full_policy), CompletionPass::Descriptions);

    sim_assert_eq!(have: full_first.schema, want: full_second.schema);
    sim_assert_eq!(
        have: full_first.emission_report,
        want: full_second.emission_report
    );
    sim_assert_eq!(have: lean_first.schema, want: lean_second.schema);
    sim_assert_eq!(
        have: lean_first.emission_report,
        want: lean_second.emission_report
    );

    for report in [&full_first.emission_report, &lean_first.emission_report] {
        sim_assert_eq!(
            have: report.facts.lowered,
            want: report.facts.selected + report.facts.dropped
        );
        sim_assert_eq!(
            have: report.counts_for_class(EmissionClassKind::Mandatory).dropped,
            want: 0
        );
    }
    for class in [
        EmissionClassKind::OrdinaryRoot,
        EmissionClassKind::KindPartitionRoot,
        EmissionClassKind::KindPartitionLocal,
        EmissionClassKind::TerminalAlways,
        EmissionClassKind::TerminalGuarded,
    ] {
        sim_assert_eq!(
            have: lean_first
                .emission_report
                .counts_for_class(class)
                .selected,
            want: 0
        );
    }
}

#[test]
fn projections_never_reenter_the_provider() {
    #[derive(Debug, Default)]
    struct CountingProvider {
        calls: AtomicUsize,
    }

    impl ResourceSchemaOracle for CountingProvider {
        fn schema_fragment_for_use(
            &self,
            _use_: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    let source = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          replicas: {{ .Values.replicas }}
    "};
    let values_yaml = "replicas: 1\n";
    let signals = schema_signals_for(parse_ir(source));
    let provider = CountingProvider::default();
    let input = ValuesSchemaInput::new(&signals, &provider).with_values_yaml(Some(values_yaml));
    let plan = LoweredEmissionPlan::build(&input);
    let calls_after_lowering = provider.calls.load(Ordering::Relaxed);
    assert!(calls_after_lowering > 0);

    let full_policy = EmissionPolicy::for_profile(SchemaProfile::Full);
    let lean_policy = EmissionPolicy::for_profile(SchemaProfile::Lean);
    let _ = plan.complete(plan.project(full_policy), CompletionPass::Descriptions);
    let _ = plan.complete(plan.project(lean_policy), CompletionPass::Descriptions);

    sim_assert_eq!(
        have: provider.calls.load(Ordering::Relaxed),
        want: calls_after_lowering
    );
}
