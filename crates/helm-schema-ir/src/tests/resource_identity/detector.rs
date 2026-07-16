use helm_schema_ast::DefineIndex;
use indoc::indoc;

use crate::analysis_db::IrAnalysisDb;
use crate::{CapabilityGuard, HelperBranchBody};
use test_util::prelude::sim_assert_eq;

fn collect_spans(src: &str, analysis_db: &IrAnalysisDb) -> Vec<helm_schema_ast::ResourceSpan> {
    let document = helm_schema_syntax::TemplatedDocument::parse(src);
    crate::resource_identity::collect_resource_spans(&document, analysis_db)
}

fn detect(src: &str, defines: &DefineIndex) -> Option<crate::ResourceRef> {
    let analysis_db = IrAnalysisDb::new(defines);
    collect_spans(src, &analysis_db)
        .into_iter()
        .next()
        .map(|span| span.resource)
}

#[test]
fn detects_kind_before_api_version() {
    let resource = detect(
        indoc! {r#"
            kind: NetworkPolicy
            apiVersion: networking.k8s.io/v1
            metadata:
              name: example
        "#},
        &DefineIndex::new(),
    )
    .expect("resource");

    sim_assert_eq!(have: resource.kind, want: "NetworkPolicy");
    sim_assert_eq!(have: resource.api_version, want: "networking.k8s.io/v1");
}

#[test]
fn preserves_inline_conditional_kind_candidates() {
    let resource = detect(
        indoc! {r#"
            apiVersion: apps/v1
            kind: {{ if .Values.persistence }}StatefulSet{{ else }}Deployment{{ end }}
            metadata:
              name: example
        "#},
        &DefineIndex::new(),
    )
    .expect("resource");

    sim_assert_eq!(have: resource.kind, want: "StatefulSet");
    sim_assert_eq!(have: resource.kind_candidates, want: vec!["Deployment"]);
}

#[test]
fn recovers_values_selected_kind_candidates_from_body_partitions() {
    let resource = detect(
        indoc! {r#"
            apiVersion: apps/v1
            kind: {{ .Values.workloadKind }}
            metadata:
              name: example
            spec:
              {{- if not (eq .Values.workloadKind "DaemonSet") }}
              replicas: 1
              {{- end }}
              {{- if eq .Values.workloadKind "StatefulSet" }}
              serviceName: example
              {{- end }}
              {{- if eq .Values.workloadKind "Deployment" }}
              strategy: {}
              {{- end }}
        "#},
        &DefineIndex::new(),
    )
    .expect("resource");

    sim_assert_eq!(have: resource.kind, want: "DaemonSet");
    sim_assert_eq!(
        have: resource.kind_candidates,
        want: vec!["StatefulSet", "Deployment"]
    );
}

#[test]
fn detects_resources_inside_template_control_bodies_after_preamble() {
    let source = indoc! {r#"
        {{- $name := include "x.name" . }}
        {{- if .Values.create }}
        apiVersion: v1
        kind: Secret
        metadata:
          name: {{ $name }}
        {{- end }}
        {{- if .Values.extra }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ $name }}-extra
        {{- end }}
    "#};
    let defines = DefineIndex::new();
    let analysis_db = IrAnalysisDb::new(&defines);
    let spans = collect_spans(source, &analysis_db);

    sim_assert_eq!(have: spans.len(), want: 2);
    sim_assert_eq!(have: spans[0].start, want: 0);
    sim_assert_eq!(have: spans[0].resource.kind.as_str(), want: "Secret");
    sim_assert_eq!(have: spans[1].resource.kind.as_str(), want: "ConfigMap");
}

#[test]
fn detects_resources_in_signoz_postgresql_secrets_template() {
    let source = include_str!(
        "../../../../../testdata/charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml"
    );
    let defines = DefineIndex::new();
    let analysis_db = IrAnalysisDb::new(&defines);
    let spans = collect_spans(source, &analysis_db);

    sim_assert_eq!(have: spans.len(), want: 3);
    assert!(spans.iter().all(|span| span.resource.kind == "Secret"));
}

#[test]
fn resolves_helper_returned_api_version() {
    let helpers = indoc! {r#"
        {{- define "x.apiVersion" -}}
        {{- print "apps/v1" -}}
        {{- end -}}
    "#};
    let mut defines = DefineIndex::new();
    defines.add_file_source("<inline:0>", helpers);
    let resource = detect(
        indoc! {r#"
            apiVersion: {{ template "x.apiVersion" . }}
            kind: Deployment
            metadata:
              name: example
        "#},
        &defines,
    )
    .expect("resource");

    sim_assert_eq!(have: resource.kind, want: "Deployment");
    sim_assert_eq!(have: resource.api_version, want: "apps/v1");
    assert!(resource.api_version_candidates.is_empty());
}

#[test]
fn preserves_inline_capability_branches() {
    let resource = detect(
        indoc! {r#"
            {{- if .Capabilities.APIVersions.Has "policy/v1" }}
            apiVersion: policy/v1
            {{- else }}
            apiVersion: policy/v1beta1
            {{- end }}
            kind: PodDisruptionBudget
            metadata:
              name: example
        "#},
        &DefineIndex::new(),
    )
    .expect("resource");

    sim_assert_eq!(have: resource.kind, want: "PodDisruptionBudget");
    sim_assert_eq!(have: resource.api_version, want: "policy/v1");
    sim_assert_eq!(
        have: resource.api_version_candidates,
        want: vec!["policy/v1beta1".to_string()]
    );
    sim_assert_eq!(have: resource.api_version_branches.len(), want: 2);
    sim_assert_eq!(
        have: resource.api_version_branches[0].guard,
        want: Some(CapabilityGuard::Has {
            api: "policy/v1".to_string()
        })
    );
    sim_assert_eq!(
        have: resource.api_version_branches[1].body,
        want: HelperBranchBody::literals(vec!["policy/v1beta1".to_string()])
    );
}

#[test]
fn preserves_semver_gated_api_version_branches() {
    let resource = detect(
        indoc! {r#"
            {{- if .Values.ingress.enabled -}}
            {{- if semverCompare ">=1.19-0" .Capabilities.KubeVersion.GitVersion -}}
            apiVersion: networking.k8s.io/v1
            {{- else if semverCompare ">=1.14-0" .Capabilities.KubeVersion.GitVersion -}}
            apiVersion: networking.k8s.io/v1beta1
            {{- else -}}
            apiVersion: extensions/v1beta1
            {{- end }}
            kind: Ingress
            {{- end }}
        "#},
        &DefineIndex::new(),
    )
    .expect("resource");

    sim_assert_eq!(have: resource.kind, want: "Ingress");
    sim_assert_eq!(have: resource.api_version, want: "networking.k8s.io/v1");
    sim_assert_eq!(
        have: resource.api_version_candidates,
        want: vec![
            "networking.k8s.io/v1beta1".to_string(),
            "extensions/v1beta1".to_string(),
        ]
    );
}

#[test]
fn preserves_zalando_ingress_api_version_branches_with_later_ranges() {
    let source = include_str!(
        "../../../../../testdata/charts/zalando-postgres-operator-ui/templates/ingress.yaml"
    );
    let resource = detect(source, &DefineIndex::new()).expect("resource");

    sim_assert_eq!(have: resource.kind, want: "Ingress");
    sim_assert_eq!(have: resource.api_version, want: "networking.k8s.io/v1");
    sim_assert_eq!(
        have: resource.api_version_candidates,
        want: vec![
            "networking.k8s.io/v1beta1".to_string(),
            "extensions/v1beta1".to_string(),
        ]
    );
}

#[test]
fn mixed_literal_and_nested_branch_preserves_nested_guards() {
    let resource = detect(
        indoc! {r#"
            {{- if .Capabilities.APIVersions.Has "policy/v1" }}
            apiVersion: policy/v1
            {{- if .Capabilities.APIVersions.Has "policy/v1/PodDisruptionBudget" }}
            apiVersion: policy/v1
            {{- else }}
            apiVersion: policy/v1beta1
            {{- end }}
            {{- else }}
            apiVersion: policy/v1beta1
            {{- end }}
            kind: PodDisruptionBudget
        "#},
        &DefineIndex::new(),
    )
    .expect("resource");

    let HelperBranchBody::Nested { branches } = &resource.api_version_branches[0].body else {
        panic!("expected nested branch body");
    };
    sim_assert_eq!(have: branches.len(), want: 3);
    sim_assert_eq!(
        have: branches[0].body,
        want: HelperBranchBody::literals(vec!["policy/v1".to_string()])
    );
    sim_assert_eq!(
        have: branches[1].guard,
        want: Some(CapabilityGuard::Has {
            api: "policy/v1/PodDisruptionBudget".to_string()
        })
    );
    sim_assert_eq!(
        have: branches[2].body,
        want: HelperBranchBody::literals(vec!["policy/v1beta1".to_string()])
    );
}

#[test]
fn capability_guard_without_api_version_does_not_create_empty_branch_resource() {
    let resource = detect(
        indoc! {r#"
            {{- if .Capabilities.APIVersions.Has "v1/ConfigMap" }}
            metadata:
              labels:
                enabled: "true"
            {{- end }}
            apiVersion: v1
            kind: ConfigMap
        "#},
        &DefineIndex::new(),
    )
    .expect("resource");

    sim_assert_eq!(have: resource.kind, want: "ConfigMap");
    sim_assert_eq!(have: resource.api_version, want: "v1");
    assert!(resource.api_version_candidates.is_empty());
    assert!(resource.api_version_branches.is_empty());
}

#[test]
fn helper_output_under_guarded_resource_does_not_become_api_version_candidate() {
    let helpers = indoc! {r#"
        {{- define "x.labels" -}}
        app.kubernetes.io/name: example
        app.kubernetes.io/instance: test
        {{- end -}}
    "#};
    let mut defines = DefineIndex::new();
    defines.add_file_source("<inline:0>", helpers);
    let resource = detect(
        indoc! {r#"
            {{- if .Values.networkPolicy.enabled }}
            apiVersion: networking.k8s.io/v1
            kind: NetworkPolicy
            metadata:
              labels:
                {{- include "x.labels" . | nindent 4 }}
            {{- end }}
        "#},
        &defines,
    )
    .expect("resource");

    sim_assert_eq!(have: resource.api_version, want: "networking.k8s.io/v1");
    assert!(resource.api_version_candidates.is_empty());
    assert!(resource.api_version_branches.is_empty());
}
