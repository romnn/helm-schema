use crate::analysis_db::IrAnalysisDb;
use crate::value::execute_document;
use helm_schema_ast::DefineIndex;
use helm_schema_core::Predicate;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn exact_text(source: &str, invocation: &str) -> Vec<Option<String>> {
    let mut defines = DefineIndex::new();
    defines.add_file_source("chart/templates/_helpers.tpl", source);
    let db = IrAnalysisDb::new(&defines);
    let execution = execute_document(invocation, Some("chart/templates/test.yaml"), &db);
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    execution
        .text()
        .map(|(_, text)| text.exact_text())
        .collect()
}

#[test]
fn helper_and_region_boundaries_apply_their_own_trim_markers() {
    for body in [
        indoc! {r#"
            {{- define "probe" -}}
            hello
            {{- end -}}
        "#},
        indoc! {r#"
            {{ define "probe" }}{{- if true -}}
            hello
            {{- end -}}{{ end }}
        "#},
        indoc! {r#"
            {{- define "probe" -}}
            {{ if true -}}
            hello
            {{- end }}
            {{- end -}}
        "#},
        indoc! {r#"
            {{- define "probe" -}}
            {{ if false }}wrong{{ else -}}
            hello
            {{- end }}
            {{- end -}}
        "#},
        indoc! {r#"
            {{- define "probe" -}}
            {{ with (dict "x" "hello") -}}
            {{ .x }}
            {{- end }}
            {{- end -}}
        "#},
    ] {
        sim_assert_eq!(have: exact_text(body, r#"{{ include "probe" . }}"#), want: vec![Some("hello".to_owned())], "{body}");
    }
}

#[test]
fn range_each_iteration_uses_lexical_boundary_trims() {
    let source = indoc! {r#"
        {{- define "probe" -}}
        {{ range (list "1" "2") -}}
        {{ . }}
        {{- end -}}
        {{- end -}}
    "#};
    sim_assert_eq!(have: exact_text(source, r#"{{ include "probe" . }}"#), want: vec![Some("12".to_owned())]);
}

#[test]
fn region_suffix_uses_end_markers_instead_of_opener_markers() {
    for (source, expected) in [
        ("A{{ if true -}} B {{ end }} C", "AB  C"),
        ("A{{ if true }} B {{ end -}} C", "A B C"),
        ("A{{ if false -}} B {{ else }} C {{ end }} D", "A C  D"),
        ("A{{ if false }} B {{ else -}} C {{ end -}} D", "AC D"),
    ] {
        sim_assert_eq!(have: exact_text("", source), want: vec![Some(expected.to_owned())], "{source}");
    }
}

#[test]
fn trim_preserves_unicode_whitespace_and_adjusts_source_spans() {
    let mut source = String::from("A\u{a0} \t{{- if true -}} \t\u{a0}B\u{2028} \r");
    source.push('\n');
    source.push_str("{{- end -}} \t\u{85}C");
    sim_assert_eq!(have: exact_text("", &source), want: vec![Some("A\u{a0}\u{a0}B\u{2028}\u{85}C".to_owned())]);
}

#[test]
fn absent_trim_markers_keep_helper_whitespace_and_empty_definitions() {
    let plain = indoc! {"{{ define \"probe\" }}\n hello \n{{ end }}\n"};
    sim_assert_eq!(have: exact_text(plain, r#"{{ include "probe" . }}"#), want: vec![Some("\n hello \n".to_owned())]);
    let mut empty = String::from("{{- define \"probe\" -}} \t\r");
    empty.push('\n');
    empty.push_str("{{- end -}}");
    sim_assert_eq!(have: exact_text(&empty, r#"{{ include "probe" . }}"#), want: vec![Some(String::new())]);
}

#[test]
fn boolean_helper_trim_retains_both_guarded_text_alternatives() {
    let source = indoc! {r#"
        {{- define "probe" -}}
        {{- if .Values.enabled -}}
        true
        {{- else -}}
        false
        {{- end -}}
        {{- end -}}
    "#};
    let mut defines = DefineIndex::new();
    defines.add_file_source("chart/templates/_helpers.tpl", source);
    let db = IrAnalysisDb::new(&defines);
    for (literal, expected) in [
        ("true", Predicate::truthy_path("enabled")),
        ("false", Predicate::truthy_path("enabled").negated()),
    ] {
        let invocation = format!(
            r#"{{{{ if eq (include "probe" .) "{literal}" }}}}{{{{ fail "selected" }}}}{{{{ end }}}}"#
        );
        let execution = execute_document(&invocation, None, &db);
        sim_assert_eq!(have: execution.abort_predicate(), want: expected, "{literal}");
    }
}

#[test]
fn block_invokes_its_body_with_the_pipeline_as_dot_even_when_false() {
    for (source, expected) in [
        (
            r#"{{ block "probe" (dict "x" "inner") }}{{ .x }}|{{ $.x }}{{ end }}"#,
            "inner|inner",
        ),
        (
            r#"{{ block "probe" false }}{{ if . }}wrong{{ else }}false{{ end }}{{ end }}"#,
            "false",
        ),
        (
            r#"{{ block "probe" (dict "a" "x") }}{{ if .Values }}{{ fail "outer root leaked" }}{{ end }}{{ end }}"#,
            "",
        ),
    ] {
        sim_assert_eq!(have: exact_text(source, source), want: vec![Some(expected.to_owned())], "{source}");
    }
    let source = r#"{{ block "probe" false }}{{ fail "body still runs" }}{{ end }}"#;
    let mut defines = DefineIndex::new();
    defines.add_file_source("chart/templates/_helpers.tpl", source);
    let db = IrAnalysisDb::new(&defines);
    sim_assert_eq!(have: execute_document(source, None, &db).abort_predicate(), want: Predicate::True);
}
