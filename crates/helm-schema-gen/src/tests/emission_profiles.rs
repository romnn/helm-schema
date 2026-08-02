use super::*;
use crate::{
    CompletionPass, SchemaProfile, generate_values_schema_through,
    generate_values_schema_with_report,
};
use color_eyre::eyre;
use test_util::prelude::sim_assert_eq;

#[test]
fn lean_profile_omits_only_document_level_conditionals() {
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
        sim_assert_eq!(
            have: report.projected_facts.lowered,
            want: report.projected_facts.selected + report.projected_facts.dropped
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
    let projected_mandatory =
        lean.projected_counts_for_class(crate::emission_policy::EmissionClassKind::Mandatory);
    sim_assert_eq!(have: projected_mandatory.dropped, want: 0);
    sim_assert_eq!(
        have: lean
            .counts_for_class(crate::emission_policy::EmissionClassKind::Mandatory)
            .selected,
        want: 0
    );
    sim_assert_eq!(have: lean.mandatory_outcomes.total(), want: 0);
    assert!(full.facts.selected > lean.facts.selected);
    for class in [
        crate::emission_policy::EmissionClassKind::Mandatory,
        crate::emission_policy::EmissionClassKind::OrdinaryRoot,
        crate::emission_policy::EmissionClassKind::OrdinaryLocal,
        crate::emission_policy::EmissionClassKind::KindPartitionRoot,
        crate::emission_policy::EmissionClassKind::KindPartitionLocal,
        crate::emission_policy::EmissionClassKind::TerminalAlways,
        crate::emission_policy::EmissionClassKind::TerminalGuarded,
    ] {
        sim_assert_eq!(have: lean.counts_for_class(class).selected, want: 0);
    }
    assert!(!lean.selection_differences.is_empty());
}

#[test]
fn kind_partition_policy_requires_at_least_one_anchor_lane() {
    use crate::emission_policy::{EmissionKnobs, EmissionPolicy};

    let policy = |root, local, kind| {
        EmissionPolicy::new(EmissionKnobs {
            root_anchored_conditionals: root,
            local_conditionals: local,
            terminal_clauses: true,
            kind_partitions: kind,
        })
    };
    assert!(policy(false, true, true).is_valid());
    assert!(policy(true, false, true).is_valid());
    assert!(!policy(false, false, true).is_valid());
    assert!(policy(false, false, false).is_valid());
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
        let (full, _) = generate_values_schema_through(
            &ValuesSchemaInput::new(&signals, &NoopProvider)
                .with_values_yaml(Some(values_yaml))
                .with_profile(SchemaProfile::Full),
            pass,
        );
        let (lean, _) = generate_values_schema_through(
            &ValuesSchemaInput::new(&signals, &NoopProvider)
                .with_values_yaml(Some(values_yaml))
                .with_profile(SchemaProfile::Lean),
            pass,
        );
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
