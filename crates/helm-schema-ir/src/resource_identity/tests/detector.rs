use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use indoc::indoc;

use super::super::ResourceIdentityDetector;
use crate::capability_branch::{CapabilityGuard, HelperBranchBody};
use test_util::prelude::sim_assert_eq;

fn detect(src: &str, defines: &DefineIndex) -> Option<crate::ResourceRef> {
    let ast = TreeSitterParser.parse(src).expect("parse template");
    ResourceIdentityDetector::new(defines).detect(&ast)
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
fn resolves_helper_returned_api_version() {
    let helpers = indoc! {r#"
        {{- define "x.apiVersion" -}}
        {{- print "apps/v1" -}}
        {{- end -}}
    "#};
    let mut defines = DefineIndex::new();
    defines
        .add_source(&TreeSitterParser, helpers)
        .expect("helpers");
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
    sim_assert_eq!(have: resource.api_version, want: "");
    sim_assert_eq!(
        have: resource.api_version_candidates,
        want: vec!["policy/v1".to_string(), "policy/v1beta1".to_string()]
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
