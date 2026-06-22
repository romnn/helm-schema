use super::{
    extract_define_blocks, extract_helper_calls, extract_helper_calls_from_ast_body,
    extract_helper_calls_from_ast_excluding_defines,
};
use helm_schema_ast::{HelmParser, TreeSitterParser};
use test_util::prelude::sim_assert_eq;

#[test]
fn extracts_define_blocks_with_exact_body_spans() {
    let src = indoc::indoc! {r#"
        {{- define "outer" -}}
        before
        {{- define "inner" -}}
        inside
        {{- end -}}
        after
        {{- end -}}
    "#};

    let blocks = extract_define_blocks(src);
    sim_assert_eq!(have: blocks.len(), want: 2);
    sim_assert_eq!(have: blocks[0].name, want: "outer");
    sim_assert_eq!(have: blocks[1].name, want: "inner");
    sim_assert_eq!(have: &src[blocks[0].body_range.clone()], want: blocks[0].body);
    sim_assert_eq!(have: &src[blocks[1].body_range.clone()], want: blocks[1].body);
    assert!(blocks[0].body.contains("before"));
    assert!(blocks[0].body.contains("after"));
    assert!(blocks[0].body.contains(r#"{{- define "inner" -}}"#));
    sim_assert_eq!(have: blocks[1].body.trim(), want: "inside");
}

#[test]
fn extracts_define_blocks_without_comment_masking_heuristics() {
    let src = indoc::indoc! {r#"
        {{- define "x" -}}
        {{/* {{ end }} should not terminate the define */}}
        value
        {{- end -}}
    "#};

    let blocks = extract_define_blocks(src);
    sim_assert_eq!(have: blocks.len(), want: 1);
    sim_assert_eq!(have: blocks[0].name, want: "x");
    assert!(blocks[0].body.contains("should not terminate"));
    sim_assert_eq!(
        have: &src[blocks[0].byte_range.clone()],
        want: src.get(blocks[0].byte_range.clone()).unwrap_or("")
    );
}

#[test]
fn extracts_real_include_call() {
    let src = r#"{{ include "common.labels" . }}"#;
    sim_assert_eq!(have: extract_helper_calls(src), want: vec!["common.labels".to_string()]);
}

#[test]
fn extracts_real_template_call() {
    let src = r#"{{ template "common.labels" . }}"#;
    sim_assert_eq!(have: extract_helper_calls(src), want: vec!["common.labels".to_string()]);
}

#[test]
fn skips_helm_comment_call() {
    let src = r#"{{/* include "common.fake" */}}{{ include "common.real" . }}"#;
    sim_assert_eq!(have: extract_helper_calls(src), want: vec!["common.real".to_string()]);
}

#[test]
fn skips_call_inside_double_quoted_string() {
    let src = r#"{{ "include \"common.fake\"" | quote }}{{ include "common.real" . }}"#;
    sim_assert_eq!(have: extract_helper_calls(src), want: vec!["common.real".to_string()]);
}

#[test]
fn skips_call_inside_backtick_raw_string() {
    let src = "{{ `include \"common.fake\"` | quote }}{{ include \"common.real\" . }}";
    sim_assert_eq!(have: extract_helper_calls(src), want: vec!["common.real".to_string()]);
}

#[test]
fn multiple_real_calls_in_one_action() {
    let src = r#"{{ include "a" . }}{{ include "b" . }}"#;
    sim_assert_eq!(
        have: extract_helper_calls(src),
        want: vec!["a".to_string(), "b".to_string()],
    );
}

#[test]
fn dedup_preserves_first_occurrence_order() {
    let src = r#"{{ include "a" . }}{{ include "b" . }}{{ include "a" . }}"#;
    sim_assert_eq!(
        have: extract_helper_calls(src),
        want: vec!["a".to_string(), "b".to_string()],
    );
}

#[test]
fn extracts_helper_inside_control_flow_body() {
    let src = r#"{{ if .X }}{{ include "deep" . }}{{ end }}"#;
    sim_assert_eq!(have: extract_helper_calls(src), want: vec!["deep".to_string()]);
}

#[test]
fn extracts_helper_inside_range_destructure_header() {
    let src = r#"{{ range $i, $v := include "src" . }}{{ end }}"#;
    sim_assert_eq!(have: extract_helper_calls(src), want: vec!["src".to_string()]);
}

#[test]
fn ast_extraction_can_skip_define_bodies_for_chart_direct_calls() {
    let src = indoc::indoc! {r#"
        {{ include "direct" . }}
        {{- define "helper" -}}
        {{ include "nested" . }}
        {{- end -}}
    "#};
    let ast = TreeSitterParser.parse(src).expect("parse");

    sim_assert_eq!(
        have: extract_helper_calls_from_ast_excluding_defines(&ast),
        want: vec!["direct".to_string()]
    );
}

#[test]
fn ast_body_extraction_visits_helper_body_headers_and_actions() {
    let src = indoc::indoc! {r#"
        {{- define "helper" -}}
        {{- if include "guard" . -}}
        {{ include "body" . }}
        {{- end -}}
        {{- end -}}
    "#};
    let ast = TreeSitterParser.parse(src).expect("parse");
    let helm_schema_ast::HelmAst::Document { items } = ast else {
        panic!("expected document root");
    };
    let [helm_schema_ast::HelmAst::Define { body, .. }] = items.as_slice() else {
        panic!("expected one define");
    };

    sim_assert_eq!(
        have: extract_helper_calls_from_ast_body(body),
        want: vec!["guard".to_string(), "body".to_string()]
    );
}

#[test]
fn ast_extraction_finds_helper_calls_embedded_inside_scalar_text() {
    let src = indoc::indoc! {r#"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: {{ include "helper.name" . }}-suffix
    "#};
    let ast = TreeSitterParser.parse(src).expect("parse");

    sim_assert_eq!(
        have: extract_helper_calls_from_ast_excluding_defines(&ast),
        want: vec!["helper.name".to_string()]
    );
}
