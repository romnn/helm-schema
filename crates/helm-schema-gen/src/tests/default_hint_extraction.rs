use super::*;

/// Step 2: negative-integer literal still recognised, type hint is integer.
#[test]
fn step2_default_negative_integer_literal() {
    let src = indoc! {r"
        replicas: {{ default -3 .Values.replicas }}
    "};
    let hints = type_hints_for(parse_ir(src));
    let schemas = hints.get("replicas").expect("replicas hint present");
    assert!(
        schemas.contains("integer"),
        "expected integer hint for negative literal, got {schemas:?}"
    );
}

/// Step 2: rooted `$.Values.X` and `$root.Values.X` forms (used inside
/// ranges/withs where `.` is rebound) are recognised too — not just the
/// plain `.Values.X` form.
#[test]
fn step2_default_rooted_values_paths_recognised() {
    let src = indoc! {r#"
        {{- range .Values.servers }}
        name: {{ default "alertmanager" $.Values.alertmanager.nameOverride }}
        alias: {{ default "main" $root.Values.alertmanager.aliasOverride }}
        {{- end }}
    "#};
    let hints = type_hints_for(parse_ir(src));
    assert!(
        hints.contains_key("alertmanager.nameOverride"),
        "expected hint for $.Values.alertmanager.nameOverride, got {hints:?}"
    );
    assert!(
        hints.contains_key("alertmanager.aliasOverride"),
        "expected hint for $root.Values.alertmanager.aliasOverride, got {hints:?}"
    );
}

/// Step 2 false-positive guard: a `default` pattern inside a YAML comment
/// MUST NOT produce a type hint. (Acceptable known limitation if it does —
/// document with a SKIP marker — but flag the case explicitly.)
#[test]
fn step2_default_in_yaml_comment_no_hint() {
    let src = indoc! {r#"
        # example: {{ default "x" .Values.exampleName }}
        name: actual
    "#};
    let hints = type_hints_for(parse_ir(src));
    assert!(
        hints.is_empty(),
        "YAML comments must not produce hints, got {hints:?}"
    );
}

/// Step 2 false-positive guard: a `default` pattern inside a Helm template
/// comment (`{{/* ... */}}`) MUST NOT produce a type hint.
#[test]
fn step2_default_in_helm_comment_no_hint() {
    let src = indoc! {r#"
        {{/* default "x" .Values.exampleName */}}
        name: actual
    "#};
    let hints = type_hints_for(parse_ir(src));
    assert!(
        hints.is_empty(),
        "Helm comments must not produce hints, got {hints:?}"
    );
}

/// Step 2 false-positive guard: a `default` pattern inside a Go string
/// literal embedded in a template MUST NOT produce a type hint.
#[test]
fn step2_default_in_string_literal_no_hint() {
    // A real chart might emit a doc string mentioning the syntax it
    // supports. The extractor must not be fooled by syntax that's text data.
    let src = indoc! {r#"
        docs: {{- "see: default 5 .Values.example" | quote }}
    "#};
    let hints = type_hints_for(parse_ir(src));
    assert!(
        hints.is_empty(),
        "Go-string-literal text must not produce hints, got {hints:?}"
    );
}

/// Strict per-use rule for contract nullable-path facts: a path is
/// only null-tolerant when *every* render use carries a null-tolerating
/// guard. Two uses of the same source expression - one with
/// `Guard::Default { path }` matching, one with no guards - must not
/// widen the path. Renders that hit the bare site would crash on null,
/// so the schema must reject null too.
///
/// This locks in the design line called out in review: do not widen a
/// path on the strength of "any single use has a Default guard." Only
/// the structural set-mutation pattern in a helper (see
/// `SymbolicWalker::set_default_chart_paths_for_text`) propagates the
/// guard to every read that runs after the mutation; under the strict
/// per-use rule, that path correctly widens. Mixed-guards paths stay
/// strict.
#[test]
fn contract_ir_nullable_paths_require_all_render_uses_to_be_null_tolerant() {
    let guarded = ContractUse {
        source_expr: "image.tag".into(),
        path: YamlPath(vec!["data".into(), "guarded".into()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Default {
            path: "image.tag".into(),
        }]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
    };
    let bare = ContractUse {
        source_expr: "image.tag".into(),
        path: YamlPath(vec!["data".into(), "bare".into()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
    };

    let signals = schema_signals_for(vec![guarded, bare]);
    let null_paths = signals
        .schema_evidence_by_value_path()
        .iter()
        .filter(|(_, evidence)| evidence.facts.is_nullable)
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        null_paths.is_empty(),
        "image.tag must not be widened to nullable when one render use is unguarded; got {null_paths:?}",
    );
}

/// in a helper template (`_helpers.tpl`), not in a manifest body. The
/// temporal chart's `temporal.serviceAccountName` is the canonical case.
/// The CLI must scan helper sources too, not just manifest templates.
#[test]
fn step2_default_in_helper_template_is_extracted() {
    // Mirror the structure of the temporal chart helper: the default lives
    // inside a `define`-bound helper that gets `include`d from manifests.
    let helper_src = indoc! {r#"
        {{- define "test.serviceAccountName" -}}
        {{- if .Values.serviceAccount.create -}}
            {{ default "default-name" .Values.serviceAccount.name }}
        {{- end -}}
        {{- end -}}
    "#};
    let hints = type_hints_for(parse_ir_with_helpers(
        r#"
        name: {{ include "test.serviceAccountName" . }}
        "#,
        helper_src,
    ));
    assert!(
        hints.contains_key("serviceAccount.name"),
        "expected hint for serviceAccount.name in helper, got {hints:?}"
    );
}
