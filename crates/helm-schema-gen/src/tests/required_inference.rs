use std::collections::BTreeSet;
use test_util::prelude::sim_assert_eq;

use indoc::indoc;
use serde_json::Value;

use super::apply_required_inference;
use crate::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ast::DefineIndex;
use helm_schema_ir::{
    ContractIr, ContractUse, Guard, GuardValue, SymbolicIrContext, ValueKind, YamlPath,
};
use helm_schema_k8s::{Chain, KubernetesJsonSchemaProvider};

fn provider() -> Chain {
    Chain::new(vec![Box::new(
        KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true),
    )])
}

fn parse_contract(src: &str) -> ContractIr {
    let idx = DefineIndex::new();
    SymbolicIrContext::new(&idx).generate_contract_ir(src)
}

fn contract_for(uses: Vec<ContractUse>) -> ContractIr {
    ContractIr::from_contract_uses(uses)
}

fn generate_with_required(src: &str, values_yaml: Option<&str>) -> Value {
    let contract = parse_contract(src);
    let schema_signals = contract.finalize().into_schema_signals();
    let mut schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider()).with_values_yaml(values_yaml),
    );
    apply_required_inference(
        &mut schema,
        schema_signals.schema_evidence_by_value_path(),
        &BTreeSet::new(),
    );
    schema
}

#[test]
fn contract_default_guard_excludes_path_without_external_fallback_scan() {
    let contract = contract_for(vec![
        ContractUse {
            source_expr: "feature".to_string(),
            path: YamlPath(Vec::new()),
            kind: ValueKind::Scalar,
            guards: Vec::new(),
            resource: None,
            provenance: Vec::new(),
        },
        ContractUse {
            source_expr: "feature".to_string(),
            path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            kind: ValueKind::Scalar,
            guards: vec![Guard::Default {
                path: "feature".to_string(),
            }],
            resource: None,
            provenance: Vec::new(),
        },
    ]);
    let schema_signals = contract.finalize().into_schema_signals();
    let mut schema = generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()));

    apply_required_inference(
        &mut schema,
        schema_signals.schema_evidence_by_value_path(),
        &BTreeSet::new(),
    );

    assert!(
        schema.get("required").is_none(),
        "contract default guards should suppress required inference without a text fallback scan, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

#[test]
fn plain_pathless_scalar_use_does_not_mark_required_without_header_guard() {
    let contract = contract_for(vec![ContractUse {
        source_expr: "feature".to_string(),
        path: YamlPath(Vec::new()),
        kind: ValueKind::Scalar,
        guards: Vec::new(),
        resource: None,
        provenance: Vec::new(),
    }]);
    let schema_signals = contract.finalize().into_schema_signals();
    let mut schema = generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()));

    apply_required_inference(
        &mut schema,
        schema_signals.schema_evidence_by_value_path(),
        &BTreeSet::new(),
    );

    assert!(
        schema.get("required").is_none(),
        "plain pathless scalar uses are not enough to infer required, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

#[test]
fn explicit_nested_values_defaults_suppress_required_inference() {
    let contract = contract_for(vec![ContractUse {
        source_expr: "controller.kind".to_string(),
        path: YamlPath(Vec::new()),
        kind: ValueKind::Scalar,
        guards: vec![Guard::Eq {
            path: "controller.kind".to_string(),
            value: GuardValue::string("Deployment"),
        }],
        resource: None,
        provenance: Vec::new(),
    }]);
    let schema_signals = contract.finalize().into_schema_signals();
    let mut schema = generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()));
    let explicit_default_value_paths =
        BTreeSet::from(["controller.kind".to_string(), "controller".to_string()]);

    apply_required_inference(
        &mut schema,
        schema_signals.schema_evidence_by_value_path(),
        &explicit_default_value_paths,
    );

    assert!(
        schema.get("required").is_none(),
        "explicit nested chart defaults should suppress required inference, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

/// Guard-only feature toggles are not strong enough evidence for
/// user-requiredness: omission is a legitimate "branch disabled" choice.
#[test]
fn step3_guard_only_if_block_does_not_mark_required() {
    let src = indoc! {r"
        {{- if .Values.serviceAccount.create }}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: foo
        {{- end }}
    "};
    let schema = generate_with_required(src, None);

    assert!(
        schema.get("required").is_none(),
        "guard-only feature toggles should not become required, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

/// Step 3: paths reachable via `default <literal> .Values.X` are NOT marked
/// required, since the chart explicitly handles X being unset.
#[test]
fn step3_default_literal_excludes_path_from_required() {
    let src = indoc! {r#"
        {{- if .Values.feature }}
        foo: {{ default "x" .Values.feature }}
        {{- end }}
    "#};
    let schema = generate_with_required(src, None);

    assert!(
        schema.get("required").is_none(),
        "feature has a literal default fallback, should not be required, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

/// Step 3 regression: non-literal default fallbacks
/// (`default .Chart.Name .Values.X`) ALSO suppress required-inference.
#[test]
fn step3_default_non_literal_excludes_path_from_required() {
    let src = indoc! {r"
        {{- if .Values.nameOverride }}
        name: {{ default .Chart.Name .Values.nameOverride }}
        {{- end }}
    "};
    let schema = generate_with_required(src, None);
    assert!(
        schema.get("required").is_none(),
        "nameOverride has a non-literal default fallback, should not be required, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

/// Step 3 regression: a quoted-string-with-spaces fallback
/// (`default "two words" .Values.X`) is recognised by the fallback
/// extractor.
#[test]
fn step3_default_quoted_string_with_spaces_excludes_path_from_required() {
    let src = indoc! {r#"
        {{- if .Values.nameOverride }}
        name: {{ default "two words" .Values.nameOverride }}
        {{- end }}
    "#};
    let schema = generate_with_required(src, None);
    assert!(
        schema.get("required").is_none(),
        "nameOverride has a `default \"two words\"` fallback, should not be required, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

/// Step 3 regression: parenthesized default fallbacks
/// (`default (printf "%s-foo" .Release.Name) .Values.X`) — common in
/// fullname-style helpers — also suppress required-inference.
#[test]
fn step3_default_parenthesized_excludes_path_from_required() {
    let src = indoc! {r#"
        {{- if .Values.fullnameOverride }}
        name: {{ default (printf "%s-%s" .Release.Name "x") .Values.fullnameOverride }}
        {{- end }}
    "#};
    let schema = generate_with_required(src, None);
    assert!(
        schema.get("required").is_none(),
        "fullnameOverride has a parenthesized default fallback, should not be required, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

#[test]
fn default_after_intervening_required_call_does_not_suppress_required() {
    let src = indoc! {r#"
        {{- if .Values.name }}
        enabled: true
        {{- end }}
        name: {{ .Values.name | required "name is required" | default "fallback" }}
    "#};
    let schema = generate_with_required(src, None);
    sim_assert_eq!(
        have: schema.get("required"),
        want: Some(&serde_json::json!(["name"])),
        "default after required should not suppress required inference, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

/// Step 3 bug-fix: `if not .Values.X` must NOT mark X as required —
/// the condition fires when X is empty/null, so X being unset is
/// contractual.
#[test]
fn step3_not_guard_does_not_mark_required() {
    let src = indoc! {r"
        {{- if not .Values.legacyMode }}
        name: {{ .Values.name }}
        {{- end }}
    "};
    let schema = generate_with_required(src, None);
    assert!(
        schema.get("required").is_none(),
        "legacyMode is checked with `not`; should not be required, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

/// Step 3 bug-fix: `if or .Values.A .Values.B` must NOT mark A or B
/// as required — only one of them needs to be truthy.
#[test]
fn step3_or_guard_does_not_mark_required() {
    let src = indoc! {r"
        {{- if or .Values.primary .Values.fallback }}
        name: {{ .Values.name }}
        {{- end }}
    "};
    let schema = generate_with_required(src, None);
    assert!(
        schema.get("required").is_none(),
        "primary and fallback are an `or` pair; neither should be required, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

#[test]
fn self_guarded_helper_override_does_not_mark_required() {
    let src = indoc! {r"
        metadata:
          name: {{- if .Values.fullnameOverride -}}
            {{ .Values.fullnameOverride }}
          {{- else -}}
            generated
          {{- end -}}
    "};
    let schema = generate_with_required(src, None);
    assert!(
        schema.get("required").is_none(),
        "self-guarded helper override branches should not become required, schema={}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}

/// Sanity: applying required-inference to a schema produced WITHOUT
/// any required calls yields the same shape (modulo added `required`
/// arrays). Verifies the core gen path stays clean of required logic.
#[test]
fn core_schema_generation_yields_no_required() {
    let src = indoc! {r"
        {{- if .Values.serviceAccount.create }}
        apiVersion: v1
        kind: ServiceAccount
        {{- end }}
    "};
    let schema_signals = parse_contract(src).finalize().into_schema_signals();
    let schema = generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()));
    // The core path must never emit `required` — that's the
    // separation of concerns this module exists to enforce.
    let any_required_anywhere = serde_json::to_string(&schema)
        .unwrap()
        .contains("\"required\"");
    assert!(
        !any_required_anywhere,
        "core schema generation must not emit `required` arrays, got: {}",
        serde_json::to_string_pretty(&schema).unwrap()
    );
}
