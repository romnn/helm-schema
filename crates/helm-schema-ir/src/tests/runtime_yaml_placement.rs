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
