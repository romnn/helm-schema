use color_eyre::eyre;
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, Predicate, PredicateKind, ValuesPath};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::execute_document;

fn at<'a>(values: &'a Value, path: &ValuesPath) -> Option<&'a Value> {
    path.segments()
        .try_fold(values, |value, segment| value.get(segment.literal()?))
}

fn matches(predicate: &Predicate, values: &Value) -> eyre::Result<bool> {
    Ok(match predicate.kind() {
        PredicateKind::True => true,
        PredicateKind::False => false,
        PredicateKind::Not(inner) => !matches(inner, values)?,
        PredicateKind::And(parts) => {
            for part in parts {
                if !matches(part, values)? {
                    return Ok(false);
                }
            }
            true
        }
        PredicateKind::Or(parts) => {
            for part in parts {
                if matches(part, values)? {
                    return Ok(true);
                }
            }
            false
        }
        PredicateKind::Guard(Guard::MatchesPattern {
            path,
            pattern,
            templated: false,
        }) => {
            let pattern = regress::Regex::new(pattern)?;
            at(values, path)
                .and_then(Value::as_str)
                .is_some_and(|text| pattern.find(text).is_some())
        }
        PredicateKind::Guard(Guard::NotMatchesPattern { path, pattern }) => {
            let pattern = regress::Regex::new(pattern)?;
            at(values, path)
                .and_then(Value::as_str)
                .is_some_and(|text| pattern.find(text).is_none())
        }
        PredicateKind::Guard(Guard::Absent { path }) => at(values, path).is_none(),
        PredicateKind::Guard(Guard::Eq {
            path,
            value: helm_schema_core::GuardValue::Null,
        }) => at(values, path) == Some(&Value::Null),
        PredicateKind::Guard(Guard::Truthy { path }) => {
            at(values, path).is_some_and(|value| match value {
                Value::Null => false,
                Value::Bool(value) => *value,
                Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
                Value::String(value) => !value.is_empty(),
                Value::Array(value) => !value.is_empty(),
                Value::Object(value) => !value.is_empty(),
            })
        }
        PredicateKind::Guard(Guard::Not { path }) => {
            !matches(&Predicate::truthy_path(path.encode()), values)?
        }
        PredicateKind::Guard(Guard::TypeIs { path, schema_type }) => {
            let value = at(values, path);
            match schema_type.as_str() {
                "object" => value.is_some_and(Value::is_object),
                "string" => value.is_some_and(Value::is_string),
                "array" => value.is_some_and(Value::is_array),
                "boolean" => value.is_some_and(Value::is_boolean),
                "number" => value.is_some_and(Value::is_number),
                other => eyre::bail!("unsupported test type {other}"),
            }
        }
        other => eyre::bail!("unsupported test predicate {other:?}"),
    })
}

fn rejection(source: &str, values: &Value) -> eyre::Result<bool> {
    let mut defines = DefineIndex::new();
    defines.add_file_source("templates/probe.yaml", source);
    let db = IrAnalysisDb::new(&defines);
    let execution = execute_document(source, Some("templates/probe.yaml"), &db);
    matches(&execution.abort_predicate(), values)
}

#[test]
fn helm_decodes_only_the_first_document_in_each_manifest_chunk() -> eyre::Result<()> {
    for (separator, rejected) in [("...", false), ("---", true)] {
        let source = indoc::formatdoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            {separator}
            value: "{{{{ .Values.t }}}}"
        "#};
        sim_assert_eq!(have: rejection(&source, &json!({"t": "a\"b"}))?, want: rejected);
    }
    let source = indoc::indoc! {r#"
        value: "{{ .Values.t }}"
        ...
        [unterminated
    "#};
    sim_assert_eq!(have: rejection(source, &json!({"t": "a\"b"}))?, want: true);
    sim_assert_eq!(have: rejection(source, &json!({"t": "safe"}))?, want: false);
    Ok(())
}

#[test]
fn explicit_document_end_finalizes_only_the_first_document() -> eyre::Result<()> {
    for tail in [
        "...\n",
        indoc::indoc! {r#"
            ...
            ignored: "unterminated
        "#},
        indoc::indoc! {"
            ---
            other: safe
        "},
    ] {
        let source = indoc::formatdoc! {r#"
            value: "{{{{ .Values.t }}}}"
            {tail}
        "#};
        sim_assert_eq!(have: rejection(&source, &json!({"t": "a\\"}))?, want: true);
        sim_assert_eq!(have: rejection(&source, &json!({"t": "safe"}))?, want: false);
    }
    Ok(())
}

#[test]
fn quoted_escapes_match_helm_and_finalize_only_at_a_proven_boundary() -> eyre::Result<()> {
    let source = r#"value: "{{ .Values.t }}{{ .Values.x }}""#;
    for (text, next, rejected) in [
        (r"a\/", "", true),
        (r"a\'", "", false),
        (r"a\uD800", "", true),
        (r"a\uD7FF", "", false),
        (r"a\uE000", "", false),
        (r"a\uD", "800", true),
        (r"a\U00110000", "", true),
        (r"a\U0010FFFF", "", false),
        (r"a\U0001D800", "", false),
        (r"a\U001", "10000", true),
        ("a\\", "", true),
    ] {
        sim_assert_eq!(have: rejection(source, &json!({"t": text, "x": next}))?, want: rejected, "{text:?} + {next:?}");
    }
    // Escaping the source quote can continue into later literal text and close there.
    sim_assert_eq!(have: rejection(r#"value: "{{ .Values.t }}" suffix " # tail"#, &json!({"t": "a\\"}))?, want: false);
    Ok(())
}

#[test]
fn captured_text_preserves_splice_composition_and_frozen_inputs() -> eyre::Result<()> {
    let direct = r#"value: "{{ .Values.t }}{{ .Values.x }}""#;
    let captured = indoc::indoc! {r#"
        {{- define "inner" -}}{{ .Values.t }}{{- end -}}
        {{- define "outer" -}}{{ include "inner" . }}{{- end -}}
        {{- $text := include "outer" . -}}
        {{- $_ := set .Values "t" "safe" -}}
        value: "{{ $text }}{{ .Values.x }}"
    "#};
    for (text, next) in [
        ("a\"b", "abc"),
        (r"a\x4", "g"),
        (r"a\x4", "1"),
        ("safe", "abc"),
    ] {
        let values = json!({"t": text, "x": next});
        sim_assert_eq!(have: rejection(captured, &values)?, want: rejection(direct, &values)?, "{text:?} + {next:?}");
    }
    Ok(())
}

#[test]
fn captured_choices_keep_their_guard_when_placed_in_yaml() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        {{- define "h" -}}{{ if .Values.a }}{{ .Values.t }}{{ else }}safe{{ end }}{{- end -}}
        value: "{{ include "h" . }}"
    "#};
    sim_assert_eq!(have: rejection(source, &json!({"a": true, "t": "a\"b"}))?, want: true);
    sim_assert_eq!(have: rejection(source, &json!({"a": false, "t": "a\"b"}))?, want: false);
    Ok(())
}

#[test]
fn indentation_composes_with_captured_text_before_placement() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        {{- define "h" -}}value: "{{ .Values.t }}"{{- end -}}
        data:{{ include "h" . | indent 1 | nindent 1 }}
    "#};
    sim_assert_eq!(have: rejection(source, &json!({"t": "a\"b"}))?, want: true);
    sim_assert_eq!(have: rejection(source, &json!({"t": "safe"}))?, want: false);
    Ok(())
}

#[test]
fn stripped_captures_compose_without_reexecuting_their_inputs() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        {{- $text := tpl "{{ .Values.t }}" . -}}
        {{- $_ := set .Values "t" "safe" -}}
        value: "{{ $text }}{{ .Values.x }}"
    "#};
    for (text, next, rejected) in [
        ("a\"b", "abc", true),
        (r"a\x4", "g", true),
        (r"a\x4", "1", false),
        (r"a\x<no value>4", "1", false),
        ("safe", "abc", false),
    ] {
        sim_assert_eq!(have: rejection(source, &json!({"t": text, "x": next}))?, want: rejected);
    }
    Ok(())
}

#[test]
fn serialized_yaml_and_indentation_are_placed_before_the_next_scalar() -> eyre::Result<()> {
    for function in ["toYaml", "toYamlPretty"] {
        let source = indoc::formatdoc! {r#"
            prefix:{{{{ {function} (dict "items" (list "hello" "world")) | indent 1 | nindent 1 }}}}
            value: "{{{{ .Values.x }}}}"
        "#};
        sim_assert_eq!(have: rejection(&source, &json!({"x": "a\"b"}))?, want: true);
        sim_assert_eq!(have: rejection(&source, &json!({"x": "safe"}))?, want: false);
    }
    Ok(())
}

#[test]
fn encoded_prefixes_preserve_the_following_lexical_obligation() -> eyre::Result<()> {
    for function in ["b64enc", "sha1sum", "sha256sum", "sha512sum", "adler32sum"] {
        let source = format!(r#"value: "{{{{ .Values.t | {function} }}}}{{{{ .Values.x }}}}""#);
        for (text, next, rejected) in [("a\"b", "a\"b", true), ("a\"b", "safe", false)] {
            sim_assert_eq!(have: rejection(&source, &json!({"t": text, "x": next}))?, want: rejected, "{function}");
        }
    }
    let source =
        r#"value: "{{ ternary (b64enc .Values.t) .Values.u .Values.flag }}{{ .Values.x }}""#;
    sim_assert_eq!(have: rejection(source, &json!({"flag":true,"t":"a\"b","u":"\" # ","x":"a\"b"}))?, want: true);
    sim_assert_eq!(have: rejection(source, &json!({"flag":false,"t":"a\"b","u":"\" # ","x":"a\"b"}))?, want: false);
    Ok(())
}

#[test]
fn quoted_splices_reject_invalid_tokens_without_rejecting_completed_escapes() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          k: "{{ .Values.t }}{{ .Values.x }}"
    "#};
    // These are the direct and cross-splice admission witnesses, including main's false rejections.
    for (text, next, rejected) in [
        ("a\"b", "abc", true),
        (r"a\'b", "abc", false),
        (r"a\x4", "g", true),
        (r"a\x4", "1", false),
        (r"a\u00", "41", false),
        (r"a\U0000004", "1", false),
        (r"a\u", "0041", false),
        (r"a\x", "41", false),
        (r"a\U", "00000041", false),
    ] {
        sim_assert_eq!(have: rejection(source, &json!({"t": text, "x": next}))?, want: rejected, "{text:?} + {next:?}");
    }
    Ok(())
}

#[test]
fn single_quotes_and_frozen_aliases_keep_their_emitted_content() -> eyre::Result<()> {
    for (source, values) in [
        ("value: '{{ .Values.t }}'", json!({"t": "a'b"})),
        (
            r#"{{ $before := .Values.t }}{{ $_ := set .Values "t" "safe" }}value: "{{ $before }}""#,
            json!({"t": "a\"b"}),
        ),
    ] {
        sim_assert_eq!(have: rejection(source, &values)?, want: true, "{source}");
    }
    sim_assert_eq!(have: rejection("value: '{{ .Values.t }}'", &json!({"t": "a''b"}))?, want: false);
    Ok(())
}

#[test]
fn uncertain_prefixes_and_structure_changing_strings_do_not_prove_failure() -> eyre::Result<()> {
    // A preceding raw emission can change the YAML context; a closed quote can start valid syntax.
    for (source, values) in [
        (
            "{{ .Values.before }}value: \"{{ .Values.t }}\"",
            json!({"before": "#", "t": "a\"b"}),
        ),
        ("value: \"{{ .Values.t }}\"", json!({"t": "a\" # comment"})),
        (
            "value: \"{{ .Values.t }}{{ .Values.x }}\"",
            json!({"t": r"a\x<no value>4", "x": "1"}),
        ),
        (
            "value: \"{{ .Values.t }}{{ .Values.x }}\"",
            json!({"t": "a\"\n", "x": "other: value #"}),
        ),
    ] {
        sim_assert_eq!(have: rejection(source, &values)?, want: false, "{source}");
    }
    Ok(())
}

#[test]
fn literal_piece_boundaries_and_unknown_widening_preserve_the_failure_domain() -> eyre::Result<()> {
    let whole = r#"value: "{{ .Values.t }}""#;
    let split = r#"{{ "val" }}ue: {{ "\"" }}{{ .Values.t }}""#;
    for text in ["safe", "a\"b", r"a\x41", r"a\x4g", "a\" # comment"] {
        let values = json!({"t": text});
        sim_assert_eq!(have: rejection(split, &values)?, want: rejection(whole, &values)?);
    }
    sim_assert_eq!(have: rejection(r#"value: "{{ randAlphaNum 1 }}{{ .Values.t }}""#, &json!({"t":"a\"b"}))?, want: false);
    Ok(())
}

#[test]
fn guarded_helper_text_carries_the_selected_quote_state() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        {{- define "h" -}}{{- if .Values.a -}}\{{- end -}}{{- end -}}
        value: "{{ include "h" . }}{{ .Values.x }}"
    "#};
    sim_assert_eq!(have: rejection(source, &json!({"a": false, "x": "a\"b"}))?, want: true);
    sim_assert_eq!(have: rejection(source, &json!({"a": true, "x": "x41"}))?, want: false);
    Ok(())
}

#[test]
fn notes_are_text_and_yaml_failures_do_not_stop_the_file_schedule() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for name in ["templates/NOTES.txt", "templates/extraNOTES.txt"] {
        let notes = execute_document(r#"value: "{{ .Values.t }}""#, Some(name), &db);
        sim_assert_eq!(have: notes.abort_predicate(), want: Predicate::False);
    }

    let sources = [
        crate::symbolic::SymbolicTemplate {
            source: r#"value: "{{ .Values.t }}"{{ $_ := set .Values "after" true }}"#,
            source_path: "templates/z.yaml",
        },
        crate::symbolic::SymbolicTemplate {
            source: r#"{{ if .Values.after }}{{ fail "later file reached" }}{{ end }}"#,
            source_path: "templates/a.yaml",
        },
    ];
    let documents = crate::value::execute_documents(&sources, &db);
    // YAML decoding follows the shared-heap schedule, so the later template still aborts.
    sim_assert_eq!(have: documents[1].abort_predicate(), want: Predicate::True);
}

#[test]
fn nested_marker_removal_does_not_invent_a_resource_identity() {
    let source = indoc::indoc! {r#"
        {{ tpl "apiVersion: v1\nkind: Con<no <no value>value>figMap\nvalue: {{ .Values.t }}" . }}
    "#};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, Some("templates/probe.yaml"), &db);
    let resources = execution
        .placed_documents()
        .flat_map(|document| crate::resource_identity::placed_resources(&document, &execution))
        .map(|resource| resource.resource.kind)
        .collect::<Vec<_>>();
    sim_assert_eq!(have: resources, want: Vec::<String>::new());
}

#[test]
fn quote_operation_preserves_the_following_scalar_boundary() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        before: {{ .Values.before | quote }}
        value: "{{ .Values.t }}"
    "#};
    for before in [json!("a\"b"), json!(null), json!({"a": [1, 2]})] {
        sim_assert_eq!(have: rejection(source, &json!({"before": before, "t": "a\"b"}))?, want: true);
    }
    Ok(())
}

#[test]
fn a_manifest_failure_survives_an_uncertain_later_manifest() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        value: "{{ .Values.t }}"
        ---
        {{ if randAlphaNum 1 }}
        later: one
        {{ else }}
        later: two
        {{ end }}
    "#};
    sim_assert_eq!(have: rejection(source, &json!({"t": "a\"b"}))?, want: true);
    sim_assert_eq!(have: rejection(source, &json!({"t": "safe"}))?, want: false);
    Ok(())
}
