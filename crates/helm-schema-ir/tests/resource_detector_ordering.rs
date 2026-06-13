//! Regression tests for the resource-context path used by
//! `SymbolicIrGenerator`. The detector reads `apiVersion` / `kind` out
//! of each document header, and the locator attaches that resource
//! context to each contract claim.
//!
//! These tests pin the contract observed at the public API surface:
//! every claim produced from a templated value in the body must
//! carry both `api_version` and `kind` on its `resource`, regardless
//! of which order the two header fields appeared in.
//!
//! Round-2 regression: the detector previously gated `apiVersion`
//! parsing on `det.kind.is_none()`, so any document that wrote
//! `kind:` before `apiVersion:` silently produced `api_version=""`
//! resources. This is the exact shape Temporal's
//! `templates/network-policies.yaml` ships and was the root cause of
//! a large block of `MissingSchema(kind=..., api_version=)` noise.

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{ContractProjection, ContractUse, SymbolicIrGenerator};
use indoc::indoc;

fn generate(template: &str) -> ContractProjection {
    let idx = DefineIndex::new();
    let ast = TreeSitterParser.parse(template).expect("template parse");
    SymbolicIrGenerator.generate(template, &ast, &idx)
}

fn resource_of(use_: &ContractUse) -> (String, String) {
    let r = use_
        .resource
        .as_ref()
        .expect("contract claim must carry a resource");
    (r.api_version.clone(), r.kind.clone())
}

// Baseline: the conventional `apiVersion` THEN `kind` ordering must
// continue to work. This pins the side of the fix we didn't break.
#[test]
fn detector_records_both_when_api_version_precedes_kind() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          example: "{{ .Values.example }}"
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "example")
        .expect("expected a use for `example`");
    assert_eq!(
        resource_of(u),
        ("v1".to_string(), "ConfigMap".to_string()),
        "apiVersion-then-kind must yield (v1, ConfigMap)"
    );
}

// Pins Finding (round 3) #1 — `kind` THEN `apiVersion` is the exact
// shape that Temporal's `templates/network-policies.yaml` ships. The
// old detector dropped apiVersion in this case and produced
// `api_version=""` resources, which the chain then emitted as
// `MissingSchema(kind=NetworkPolicy, api_version=)`.
#[test]
fn detector_records_both_when_kind_precedes_api_version() {
    let template = indoc! {r#"
        kind: NetworkPolicy
        apiVersion: networking.k8s.io/v1
        metadata:
          name: temporal
        spec:
          podSelector:
            matchLabels:
              app: "{{ .Values.app }}"
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "app")
        .expect("expected a use for `app`");
    assert_eq!(
        resource_of(u),
        (
            "networking.k8s.io/v1".to_string(),
            "NetworkPolicy".to_string()
        ),
        "kind-then-apiVersion must yield (networking.k8s.io/v1, NetworkPolicy); \
         the old detector dropped apiVersion here"
    );
}

// Pins the detector's per-document reset on `---`: after a separator,
// the next document's header is re-collected from scratch. Order
// within each document must be independently free.
#[test]
fn detector_resets_at_doc_separator_and_reorders() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          first: "{{ .Values.first }}"
        ---
        kind: Secret
        apiVersion: v1
        data:
          second: "{{ .Values.second }}"
    "#};
    let ir = generate(template);

    let first = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "first")
        .expect("first use missing");
    assert_eq!(
        resource_of(first),
        ("v1".to_string(), "ConfigMap".to_string()),
        "doc 1 must resolve to ConfigMap"
    );

    let second = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "second")
        .expect("second use missing");
    assert_eq!(
        resource_of(second),
        ("v1".to_string(), "Secret".to_string()),
        "doc 2 must independently resolve to Secret regardless of header order"
    );
}

// Pins Finding (round 3) #1 — a templated `apiVersion` (`{{ … }}`)
// MUST stay unknown, not get captured as a literal string. The
// detector deliberately treats expressions as "we don't know" because
// they're not statically resolvable.
#[test]
fn detector_does_not_capture_templated_api_version() {
    let template = indoc! {r#"
        apiVersion: {{ .Values.apiVersion }}
        kind: ConfigMap
        data:
          example: "{{ .Values.example }}"
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "example")
        .expect("expected a use for `example`");
    let r = u.resource.as_ref().expect("resource on use");
    assert_eq!(r.kind, "ConfigMap", "kind must still be captured");
    assert!(
        r.api_version.is_empty(),
        "templated apiVersion must NOT be captured as a literal; got {:?}",
        r.api_version
    );
}

// Pins the analogous case: `kind: {{ .Values.kind }}` stays unknown
// even when apiVersion was captured. Catches a symmetric regression
// where we'd accidentally accept templated kind values.
#[test]
fn detector_does_not_capture_templated_kind() {
    let template = indoc! {r#"
        kind: {{ .Values.kind }}
        apiVersion: v1
        data:
          example: "{{ .Values.example }}"
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "example")
        .expect("expected a use for `example`");
    // No `resource` is produced when kind is unknown (the use is
    // anchored to the document, which has no resolved type).
    if let Some(r) = u.resource.as_ref() {
        assert!(
            r.kind.is_empty() || r.kind == "{{ .Values.kind }}".trim_matches('"'),
            "templated kind must NOT be captured as a literal; got {:?}",
            r.kind
        );
    }
}

// Pins Finding (round 4) #2 — the chart shape that's still failing in
// the real Temporal acceptance run: `kind:` first, then an `{{- if … }}`-
// wrapped `apiVersion:`. The previous detector treated the `{{- if`
// line as "non-header content" and prematurely set header_done=true,
// dropping every apiVersion that appeared after it.
#[test]
fn detector_collects_api_version_inside_if_after_kind() {
    let template = indoc! {r#"
        kind: PodDisruptionBudget
        {{- if semverCompare ">= 1.21" .Capabilities.KubeVersion.GitVersion }}
        apiVersion: policy/v1
        {{- else }}
        apiVersion: policy/v1beta1
        {{- end }}
        metadata:
          name: pdb
        spec:
          minAvailable: "{{ .Values.minAvailable }}"
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "minAvailable")
        .expect("expected a use for `minAvailable`");
    let r = u.resource.as_ref().expect("resource on use");
    assert_eq!(r.kind, "PodDisruptionBudget");
    // The detector must collect AT LEAST one of the two branches; both
    // valid. The preferred-stability rank puts `policy/v1` first.
    assert_eq!(
        r.api_version, "policy/v1",
        "preferred apiVersion must be the stable branch; got {:?} (candidates={:?})",
        r.api_version, r.api_version_candidates
    );
    // The other branch should be in the candidate list.
    assert!(
        r.api_version_candidates
            .iter()
            .any(|v| v == "policy/v1beta1"),
        "policy/v1beta1 must be in candidates from the if/else; got {:?}",
        r.api_version_candidates
    );
}

// Pins Finding (round 4) #2 — apiVersion-first variant: `apiVersion:`
// then if/else-wrapped kind would also break the same way (kind
// trapped inside an `{{- if }}` after header_done flips). Less common
// in practice but the same root cause.
#[test]
fn detector_collects_kind_inside_if_after_api_version() {
    let template = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        {{- if .Values.usePolicy }}
        kind: NetworkPolicy
        {{- else }}
        kind: Ingress
        {{- end }}
        metadata:
          name: net
        spec:
          rules: "{{ .Values.rules }}"
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "rules")
        .expect("expected a use for `rules`");
    let r = u.resource.as_ref().expect("resource on use");
    assert_eq!(r.api_version, "networking.k8s.io/v1");
    // Kind is single-value; the first one wins. Both NetworkPolicy and
    // Ingress are valid; we just need ONE captured (not empty).
    assert!(
        !r.kind.is_empty(),
        "kind must be captured even though it sits inside an {{{{- if }}}} block; got empty"
    );
}

// Pins Finding (round 4) #2 — loop-wrapped manifest. A single template
// file that emits N copies of the same resource through `{{- range … }}`
// declares the header inside the loop body. The detector must not let
// the `{{- range }}` line consume header_done.
//
// Using `.Values.commonLabels` (a top-level reference, not the loop
// variable) inside the body so the test exercises detector behaviour
// independently of the loop-scope extraction logic.
#[test]
fn detector_handles_loop_wrapped_manifest() {
    let template = indoc! {r#"
        {{- range .Values.services }}
        apiVersion: v1
        kind: Service
        metadata:
          labels: "{{ $.Values.commonLabels }}"
        ---
        {{- end }}
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "commonLabels")
        .expect("expected a use for `commonLabels` inside the loop body");
    let r = u.resource.as_ref().expect("resource on loop-body use");
    assert_eq!(r.kind, "Service");
    assert_eq!(r.api_version, "v1");
}

// Pins Finding (round 4) #2 — multi-document with `---` AND template
// actions between header lines. The detector must reset on `---` and
// then re-collect both header fields in the second doc, even when an
// `{{- if … }}` separates them.
#[test]
fn detector_multi_document_with_template_actions_between_header_lines() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          first: "{{ .Values.first }}"
        ---
        kind: NetworkPolicy
        {{- if .Values.modern }}
        apiVersion: networking.k8s.io/v1
        {{- else }}
        apiVersion: extensions/v1beta1
        {{- end }}
        spec:
          podSelector:
            matchLabels:
              app: "{{ .Values.app }}"
    "#};
    let ir = generate(template);

    let first = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "first")
        .expect("first use missing");
    assert_eq!(
        first
            .resource
            .as_ref()
            .map(|r| (r.api_version.clone(), r.kind.clone())),
        Some(("v1".to_string(), "ConfigMap".to_string())),
        "doc 1 must resolve to ConfigMap"
    );

    let app = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "app")
        .expect("app use missing");
    let r = app.resource.as_ref().expect("resource");
    assert_eq!(
        r.kind, "NetworkPolicy",
        "doc 2 must capture kind=NetworkPolicy regardless of header ordering and template actions"
    );
    assert_eq!(
        r.api_version, "networking.k8s.io/v1",
        "doc 2 must capture the stable apiVersion (preferred over extensions/v1beta1); got {:?} (candidates={:?})",
        r.api_version, r.api_version_candidates,
    );
}

// Pins Finding (round 5) #1 — helper-returned apiVersion. This is the
// exact shape used by vendored prometheus/grafana charts:
//   apiVersion: {{ template "prometheus.deployment.apiVersion" . }}
// The detector must statically resolve the helper to its literal
// output(s) so it captures apiVersion=apps/v1 instead of leaving it
// empty (which would have produced `MissingSchema(kind=Deployment,
// api_version=)` in the live Temporal acceptance run).
#[test]
fn detector_resolves_helper_returned_api_version() {
    let helpers = indoc! {r#"
        {{- define "prometheus.deployment.apiVersion" -}}
        {{- print "apps/v1" -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        apiVersion: {{ template "prometheus.deployment.apiVersion" . }}
        kind: Deployment
        metadata:
          name: x
        spec:
          replicas: {{ .Values.replicas }}
    "#};

    let mut idx = DefineIndex::new();
    idx.add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ast = TreeSitterParser.parse(template).expect("template parse");
    let ir = SymbolicIrGenerator.generate(template, &ast, &idx);

    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "replicas")
        .expect("expected use for `replicas`");
    let r = u.resource.as_ref().expect("resource on use");
    assert_eq!(r.kind, "Deployment");
    assert_eq!(
        r.api_version, "apps/v1",
        "helper must resolve to apps/v1; got {:?}",
        r.api_version
    );
}

// Pins Finding (round 5) #1 — if/else helper resolves to both branches.
// The `rbac.apiVersion` helper in vendored prometheus emits either
// `rbac.authorization.k8s.io/v1` or `rbac.authorization.k8s.io/v1beta1`
// depending on cluster capabilities. The detector must collect BOTH
// as candidates.
#[test]
fn detector_resolves_helper_with_if_else_branches() {
    let helpers = indoc! {r#"
        {{- define "rbac.apiVersion" -}}
        {{- if .Capabilities.APIVersions.Has "rbac.authorization.k8s.io/v1" -}}
        {{- print "rbac.authorization.k8s.io/v1" -}}
        {{- else -}}
        {{- print "rbac.authorization.k8s.io/v1beta1" -}}
        {{- end -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        apiVersion: {{ template "rbac.apiVersion" $ }}
        kind: RoleBinding
        metadata:
          name: x
        roleRef:
          name: {{ .Values.roleName }}
    "#};

    let mut idx = DefineIndex::new();
    idx.add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ast = TreeSitterParser.parse(template).expect("template parse");
    let ir = SymbolicIrGenerator.generate(template, &ast, &idx);

    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "roleName")
        .expect("expected use for `roleName`");
    let r = u.resource.as_ref().expect("resource on use");
    assert_eq!(r.kind, "RoleBinding");
    // Round-6 Finding 2: a helper with `{{ if … }}…{{ else }}…{{ end }}`
    // resolves to two literals whose selection is conditioned on a
    // semantic predicate (`.Capabilities.APIVersions.Has`). The IR
    // can't statically pick one without K8s capability context, so
    // it preserves BOTH alternatives in `api_version_candidates`
    // with an EMPTY primary. The chain (which has cache / version
    // chain visibility) is the version-aware phase that resolves.
    assert!(
        r.api_version.is_empty(),
        "primary must be empty for multi-branch helper (preserve exact alternatives); got {:?}",
        r.api_version
    );
    let expected = vec![
        "rbac.authorization.k8s.io/v1".to_string(),
        "rbac.authorization.k8s.io/v1beta1".to_string(),
    ];
    assert_eq!(
        r.api_version_candidates, expected,
        "both branches must be preserved in source order; got {:?}",
        r.api_version_candidates
    );
}

// Pins Finding (round 5) #1 — `include` keyword works the same as
// `template`. grafana's hpa.yaml uses `{{ include "grafana.hpa.apiVersion" . }}`.
#[test]
fn detector_resolves_include_returned_api_version() {
    let helpers = indoc! {r#"
        {{- define "grafana.hpa.apiVersion" -}}
        {{- if .Capabilities.APIVersions.Has "autoscaling/v2" -}}
        {{- print "autoscaling/v2" -}}
        {{- else -}}
        {{- print "autoscaling/v2beta2" -}}
        {{- end -}}
        {{- end -}}
    "#};
    let template = indoc! {r#"
        apiVersion: {{ include "grafana.hpa.apiVersion" . }}
        kind: HorizontalPodAutoscaler
        metadata:
          name: x
        spec:
          maxReplicas: {{ .Values.maxReplicas }}
    "#};

    let mut idx = DefineIndex::new();
    idx.add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ast = TreeSitterParser.parse(template).expect("template parse");
    let ir = SymbolicIrGenerator.generate(template, &ast, &idx);

    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "maxReplicas")
        .expect("expected use for `maxReplicas`");
    let r = u.resource.as_ref().expect("resource on use");
    assert_eq!(r.kind, "HorizontalPodAutoscaler");
    // Round-6 Finding 2: same preserve-exact-alternatives contract
    // as the rbac.apiVersion case. The version-aware chain layer
    // picks whichever branch resolves against the configured K8s
    // cache.
    assert!(
        r.api_version.is_empty(),
        "primary must be empty for multi-branch helper; got {:?}",
        r.api_version
    );
    assert_eq!(
        r.api_version_candidates,
        vec![
            "autoscaling/v2".to_string(),
            "autoscaling/v2beta2".to_string()
        ]
    );
}

// Pins Finding (round 5) #2 — source-order primary, NOT stability rank.
// PSP-style: chart writes `policy/v1beta1` (the version where PSP
// actually exists) first; a generic stability ranker would have
// flipped this to `policy/v1` and produced
// `MissingSchema(kind=PodSecurityPolicy, api_version=policy/v1)`.
#[test]
fn detector_primary_is_source_order_not_stability_rank() {
    let template = indoc! {r#"
        apiVersion: policy/v1beta1
        kind: PodSecurityPolicy
        metadata:
          name: psp
        spec:
          allowPrivilegeEscalation: {{ .Values.allowPrivilegeEscalation }}
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "allowPrivilegeEscalation")
        .expect("use missing");
    let r = u.resource.as_ref().expect("resource");
    assert_eq!(
        r.api_version, "policy/v1beta1",
        "single-apiVersion case must use what the template author wrote, NOT a stability-ranked replacement"
    );
}

// Pins Finding (round 5) #2 — multi-branch source-order primary.
// When both `policy/v1` and `policy/v1beta1` appear, the FIRST seen
// is primary (preserves authorial intent), the other is a candidate.
// Old detector picked `policy/v1` regardless of source order via
// generic "stable beats beta" ranking.
#[test]
fn detector_multi_branch_primary_is_first_seen_in_source() {
    let template = indoc! {r#"
        kind: PodDisruptionBudget
        {{- if not .Values.modern }}
        apiVersion: policy/v1beta1
        {{- else }}
        apiVersion: policy/v1
        {{- end }}
        spec:
          minAvailable: {{ .Values.minAvailable }}
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "minAvailable")
        .expect("use missing");
    let r = u.resource.as_ref().expect("resource");
    assert_eq!(
        r.api_version, "policy/v1beta1",
        "first-seen-in-source must win; the template author put v1beta1 first"
    );
    assert!(
        r.api_version_candidates.iter().any(|v| v == "policy/v1"),
        "policy/v1 must be a candidate; got {:?}",
        r.api_version_candidates
    );
}

// Pins Finding (round 4) #2 — comment lines (`#`) and other non-header
// content must not advance header_done before kind is captured.
// Bare YAML comment isn't a template directive, but it's an edge case
// often near the document head and easy to regress.
#[test]
fn detector_handles_yaml_comment_in_header() {
    let template = indoc! {r#"
        # this is a Kubernetes Service
        apiVersion: v1
        kind: Service
        spec:
          selector: "{{ .Values.selector }}"
    "#};
    let ir = generate(template);
    let u = ir
        .uses()
        .iter()
        .find(|u| u.source_expr == "selector")
        .expect("selector use missing");
    let r = u.resource.as_ref().expect("resource");
    assert_eq!(r.api_version, "v1");
    assert_eq!(r.kind, "Service");
}
