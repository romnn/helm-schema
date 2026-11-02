use std::collections::BTreeSet;
use std::fmt::Write as _;

use color_eyre::eyre;
use helm_schema_ast::DefineIndex;
use helm_schema_core::{
    CollectionDomain, CollectionQuantifier, Guard, Predicate, PredicateKind, ValuesPath,
};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;

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
        PredicateKind::Guard(Guard::MatchesValue { path, pattern }) => {
            at(values, path).is_some_and(|value| pattern.matches(value) == Some(true))
        }
        PredicateKind::Guard(Guard::CollectionPattern {
            path,
            domain,
            quantifier,
            pattern,
        }) => collection_matches(at(values, path), *domain, *quantifier, pattern)?,
        PredicateKind::Guard(Guard::MinMembers { path, bound }) => {
            at(values, path).is_some_and(|value| match value {
                Value::Object(fields) => {
                    *bound <= 0 || usize::try_from(*bound).is_ok_and(|bound| fields.len() >= bound)
                }
                _ => false,
            })
        }
        PredicateKind::Guard(Guard::AtMostOneMember { path }) => {
            at(values, path).is_some_and(|value| match value {
                Value::Array(items) => items.len() <= 1,
                Value::Object(items) => items.len() <= 1,
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
            })
        }
        PredicateKind::Guard(Guard::NotMatchesPattern { path, pattern }) => {
            let pattern = regress::Regex::new(pattern)?;
            at(values, path)
                .and_then(Value::as_str)
                .is_some_and(|text| pattern.find(text).is_none())
        }
        PredicateKind::Guard(Guard::Absent { path }) => at(values, path).is_none(),
        PredicateKind::Guard(Guard::Eq { path, value }) => {
            at(values, path) == Some(&serde_json::to_value(value)?)
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
                "null" => value.is_some_and(Value::is_null),
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
fn quote_typed_nil_map_keeps_its_printed_quotes() -> eyre::Result<()> {
    let source = r#"value: "\{{ quote (fromJson "null") }}#""#;
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);
    Ok(())
}

#[test]
fn quote_typed_nil_slice_keeps_its_printed_quotes() -> eyre::Result<()> {
    let source = r#"value: "\{{ quote (fromJsonArray "null") }}#""#;
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);
    Ok(())
}

#[test]
fn quote_nil_interface_emits_no_bytes() -> eyre::Result<()> {
    let source = r#"value: "\{{ quote .Values.absent }}#""#;
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: true);
    Ok(())
}

#[test]
fn helper_trailing_line_breaks_preserve_the_quoted_continuation() -> eyre::Result<()> {
    for source in [
        indoc::indoc! {r#"
            {{ define "h" }}{{ "\\" }}
            {{ end }}
            value: "{{ include "h" . }}{{ .Values.x }}"
        "#},
        indoc::indoc! {r#"
            {{ define "h" }}safe
            {{ end }}
            value: "{{ include "h" . }}{{ .Values.x }}"
        "#},
    ] {
        sim_assert_eq!(have: rejection(source, &json!({"x": "a\"b"}))?, want: true, "{source}");
        for value in ["abc", "\" # repaired"] {
            sim_assert_eq!(have: rejection(source, &json!({"x": value}))?, want: false, "{source}: {value}");
        }
    }
    Ok(())
}

#[test]
fn recovered_continuations_keep_helm_document_boundaries() -> eyre::Result<()> {
    for (boundary, later_rejected) in [("...", false), ("---", true)] {
        let source = indoc::formatdoc! {r#"
            {{{{ define "h" }}}}{{{{ "\\" }}}}
            {{{{ end }}}}
            value: "{{{{ include "h" . }}}}{{{{ .Values.x }}}}"
            {boundary}
            value: "{{{{ .Values.t }}}}"
        "#};
        sim_assert_eq!(have: rejection(&source, &json!({"x": "abc", "t": "a\"b"}))?, want: later_rejected, "{boundary}");
        sim_assert_eq!(have: rejection(&source, &json!({"x": "abc", "t": "safe"}))?, want: false, "{boundary}");
    }
    Ok(())
}

#[test]
fn collection_formatting_composes_member_separators_with_quoted_state() -> eyre::Result<()> {
    let source = r#"value: "{{ .Values.t }}{{ .Values.x }}""#;
    for value in [
        json!(["a\"b"]),
        json!(["ok", "a\"b"]),
        json!(["a\\x4", "b"]),
        json!({"k": "a\"b"}),
        json!({"a\\": "b"}),
    ] {
        sim_assert_eq!(have: rejection(source, &json!({"t": value, "x": "safe"}))?, want: true, "{value}");
    }
    let single = "value: '{{ .Values.t }}{{ .Values.x }}'";
    sim_assert_eq!(have: rejection(single, &json!({"t": ["a'b"], "x": "safe"}))?, want: true);
    Ok(())
}

#[test]
fn collection_formatting_retains_comment_and_separator_repairs() -> eyre::Result<()> {
    let source = r#"value: "{{ .Values.t }}{{ .Values.x }}""#;
    for values in [
        json!({"t": ["\" # ", "a\"b"]}),
        json!({"t": {"\" # ": "a\"b"}}),
        json!({"t": [["\" # ", "a\"b"]]}),
        json!({"t": {"k": ["\" # ", "a\"b"]}}),
        json!({"t": ["\" # ", "ok"], "x": "a\"b"}),
        json!({"t": ["a\\", "b"]}),
        json!({"t": {"a": "b\\", "c": "d"}}),
        json!({"t": [["a\\", "b"], "c"]}),
        json!({"t": []}),
        json!({"t": {}}),
        json!({"t": [false, 1, null]}),
    ] {
        sim_assert_eq!(have: rejection(source, &values)?, want: false, "{values}");
    }
    let single = "value: '{{ .Values.t }}{{ .Values.x }}'";
    sim_assert_eq!(have: rejection(single, &json!({"t": ["' # ", "a'b"]}))?, want: false);
    sim_assert_eq!(have: rejection(single, &json!({"t": ["a\\", "b"]}))?, want: false);
    Ok(())
}

#[test]
fn singleton_collection_closing_separator_finishes_pending_escapes() -> eyre::Result<()> {
    let source = r#"value: "{{ .Values.t }}""#;
    for value in [json!(["a\\"]), json!({"a": "b\\"})] {
        sim_assert_eq!(have: rejection(source, &json!({"t": value}))?, want: true, "{value}");
    }
    for value in [json!(["a\\", "b"]), json!({"a": "b\\", "c": "d"})] {
        sim_assert_eq!(have: rejection(source, &json!({"t": value}))?, want: false, "{value}");
    }
    Ok(())
}

#[test]
fn ordered_collection_members_compose_their_actual_separators() -> eyre::Result<()> {
    let source = r#"value: "{{ .Values.t }}{{ .Values.x }}""#;
    for values in [
        json!({"t": ["a", "b\\"]}),
        json!({"t": [["a\\"], "b"]}),
        json!({"t": ["a\\", "b\"c"]}),
        json!({"t": ["a\\", "b"], "x": "a\"b"}),
    ] {
        sim_assert_eq!(have: rejection(source, &values)?, want: true, "{values}");
    }
    for values in [
        json!({"t": ["a\\", "b"]}),
        json!({"t": [["a\\", "b"], "c"]}),
        json!({"t": ["\" # ", "b\"c"]}),
        json!({"t": ["a\\", "\" # "], "x": "a\"b"}),
    ] {
        sim_assert_eq!(have: rejection(source, &values)?, want: false, "{values}");
    }
    Ok(())
}

#[test]
fn empty_flow_collection_retains_its_following_plain_text() -> eyre::Result<()> {
    sim_assert_eq!(have: rejection("value: {{ .Values.t }}tail", &json!({"t": []}))?, want: true);
    sim_assert_eq!(have: rejection("value: {{ .Values.t }}tail", &json!({"t": "a"}))?, want: false);
    sim_assert_eq!(have: rejection("value: {{ .Values.t }}", &json!({"t": []}))?, want: false);
    sim_assert_eq!(have: rejection("{{ .Values.t }}tail", &json!({"t": []}))?, want: false);
    sim_assert_eq!(have: rejection("value: {{ .Values.t }} # tail", &json!({"t": []}))?, want: false);
    Ok(())
}

#[test]
fn captured_map_overrides_preserve_the_unmodified_input_remainder() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        {{ $v := .Values.obj }}{{ $_ := set .Values.obj "t" "ok" }}
        value: "{{ $v }}{{ .Values.x }}"
    "#};
    sim_assert_eq!(have: rejection(source, &json!({"obj": {"t": "a\"b"}, "x": "a\"b"}))?, want: true);
    sim_assert_eq!(have: rejection(source, &json!({"obj": {"t": "a\"b"}, "x": "safe"}))?, want: false);
    sim_assert_eq!(have: rejection(source, &json!({"obj": {"t": "safe", "a": "\" # "}, "x": "a\"b"}))?, want: false);
    sim_assert_eq!(have: rejection(source, &json!({"obj": {"t\n": "\" # "}, "x": "a\"b"}))?, want: false);
    Ok(())
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
fn quoted_block_values_reject_separated_suffixes() -> eyre::Result<()> {
    let source = r#"value: "{{ .Values.t }}{{ .Values.x }}""#;
    for text in ["a\", b", "a\" b", "\",", "\"]", "\"}", "\": z", "\" b"] {
        sim_assert_eq!(have: rejection(source, &json!({"t": text, "x": "abc"}))?, want: true, "{text:?}");
    }
    sim_assert_eq!(have: rejection("value: '{{ .Values.t }}'", &json!({"t": "a' 'b"}))?, want: true);
    Ok(())
}

#[test]
fn escaped_line_breaks_preserve_quoted_context_across_splices() -> eyre::Result<()> {
    let source = r#"value: "{{ .Values.t }}{{ .Values.x }}""#;
    for line_break in ['\n', '\u{85}', '\u{2028}', '\u{2029}'] {
        for (text, next, rejected) in [
            (format!("a\\{line_break}  b"), "a\"b".to_owned(), true),
            (format!("a\\{line_break}  b"), "safe".to_owned(), false),
            (format!("a\\{line_break}"), "  b\"c".to_owned(), true),
            ("a\\".to_owned(), format!("{line_break}  b\"c"), true),
            (format!("a\\{line_break}"), "#stillquoted".to_owned(), false),
        ] {
            sim_assert_eq!(have: rejection(source, &json!({"t": text, "x": next}))?, want: rejected, "{text:?} + {next:?}");
        }
    }
    Ok(())
}

#[test]
fn flow_tokens_continue_beyond_the_old_scalar_span() -> eyre::Result<()> {
    for tail in [r#""lit""#, r#""]""#, r#""{{ .Values.x }}""#] {
        let source = format!(r#"value: [ "{{{{ .Values.t }}}}", {tail} ]"#);
        sim_assert_eq!(have: rejection(&source, &json!({"t": "a\\", "x": "abc"}))?, want: true, "{tail}");
    }
    let source = r#"value: [ "{{ .Values.t }}", "{{ .Values.x }}" ]"#;
    sim_assert_eq!(have: rejection(source, &json!({"t": "\"]", "x": "abc"}))?, want: true);
    sim_assert_eq!(have: rejection(source, &json!({"t": "a\\", "x": "a\"b"}))?, want: true);
    Ok(())
}

#[test]
fn quoted_continuations_preserve_flow_keys_comments_and_unknown_boundaries() -> eyre::Result<()> {
    for (source, text, next) in [
        (
            r#"value: "{{ .Values.t }}{{ .Values.x }}""#,
            "a\" # ",
            "a\"b",
        ),
        (r#"value: "{{ .Values.t }}{{ .Values.x }}""#, "a\"#", "a\"b"),
        (
            r#"value: [ "{{ .Values.t }}", "{{ .Values.x }}" ]"#,
            "a\", \"b",
            "safe",
        ),
        (
            indoc::indoc! {r#"
            value:
              - "{{ .Values.t }}"
        "#},
            "a\": \"b",
            "",
        ),
        (r#""{{ .Values.t }}": safe"#, "key", ""),
        (r#"value: [[ "{{ .Values.t }}" ]]"#, "safe", ""),
        (r#"value: [[ "{{ .Values.t }}" ]]"#, "a\"]] # ", ""),
        (r#"value: { key: [ "{{ .Values.t }}" ] }"#, "safe", ""),
        (r#"value: [ "{{ .Values.t }}" ] suffix " ]"#, "a\\", ""),
        (
            r#"value: "{{ .Values.t }}{{ .Values.x }}""#,
            "a\\\n--",
            "-suffix\n",
        ),
        (
            r#"value: "{{ .Values.t }}{{ .Values.x }}""#,
            "a\\\n..",
            ".\n",
        ),
        (
            r#"value: "{{ .Values.t }}{{ .Values.x }}""#,
            indoc::indoc! {r#"
            a"
            ...
            ignored: "
        "#},
            "a\"b",
        ),
    ] {
        sim_assert_eq!(have: rejection(source, &json!({"t": text, "x": next}))?, want: false, "{source}: {text:?} + {next:?}");
    }
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
    let documents = crate::value::tests::execute_documents(&sources, &db);
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

#[test]
fn an_unknown_body_keeps_resource_diagnostics_without_inventing_placed_fields() {
    let source = indoc::indoc! {r"
        apiVersion: autoscaling/v2beta1
        kind: HorizontalPodAutoscaler
        metadata:
          name: {{ .Values.name | trunc 63 }}
        spec:
          replicas: {{ .Values.replicas }}
    "};
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    sim_assert_eq!(have: contract.schema_signals().observed_resources(), want: &BTreeSet::from([
        helm_schema_core::ResourceObservation {
            resource: helm_schema_core::ResourceRef::concrete("autoscaling/v2beta1".to_owned(), "HorizontalPodAutoscaler".to_owned()),
            activation: Some(vec![helm_schema_core::ConditionalGuard::TypeIs {
                path: ValuesPath::parse("name"),
                schema_type: "string".to_owned(),
            }]),
        }
    ]));
    assert!(contract.uses().iter().all(|use_| use_.resource.is_none()));
    let dormant = format!("{{{{ if false }}}}{source}{{{{ end }}}}");
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(&dormant)
        .finalize();
    assert!(contract.schema_signals().observed_resources().is_empty());
}

#[test]
fn root_flow_mapping_ignores_a_valid_token_after_its_end() -> eyre::Result<()> {
    let source =
        r#"{apiVersion: v1, kind: ConfigMap, metadata: {name: p}, data: "{{ .Values.x }}"}"#;
    sim_assert_eq!(have: rejection(source, &json!({"x": "\"}tail"}))?, want: false);
    let nested = format!("value: {source}");
    sim_assert_eq!(have: rejection(&nested, &json!({"x": "\"}tail"}))?, want: true);
    Ok(())
}

#[test]
fn independent_scalar_choices_preserve_guarded_lexical_continuations() -> eyre::Result<()> {
    let mut source = String::new();
    let mut values = serde_json::Map::new();
    for index in 0..12 {
        writeln!(
            source,
            "value{index}: \"{{{{ if .Values.enabled{index} }}}}{{{{ .Values.left{index} }}}}{{{{ else }}}}{{{{ .Values.right{index} }}}}{{{{ end }}}}\""
        )?;
        values.insert(format!("enabled{index}"), Value::Bool(index % 2 == 0));
        values.insert(format!("left{index}"), json!("safe"));
        values.insert(format!("right{index}"), json!("safe"));
    }
    source.push_str("tail: \"{{ .Values.tail }}\"\n");
    values.insert("tail".to_owned(), json!("safe"));
    sim_assert_eq!(have: rejection(&source, &Value::Object(values.clone()))?, want: false);
    values.insert("tail".to_owned(), json!("a\"b"));
    sim_assert_eq!(have: rejection(&source, &Value::Object(values.clone()))?, want: true);
    values.insert("tail".to_owned(), json!("safe"));
    values.insert("left0".to_owned(), json!("a\"b"));
    sim_assert_eq!(have: rejection(&source, &Value::Object(values.clone()))?, want: true);
    values.insert("enabled0".to_owned(), Value::Bool(false));
    sim_assert_eq!(have: rejection(&source, &Value::Object(values))?, want: false);
    Ok(())
}

#[test]
fn shared_scalar_slots_retain_guarded_resource_names() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        apiVersion: v1
        kind: {{ if .Values.outer }}{{ ternary "ConfigMap" "Secret" .Values.inner }}{{ else }}{{ ternary "Secret" "ConfigMap" .Values.inner }}{{ end }}
    "#};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, Some("templates/probe.yaml"), &db);
    let resources = execution
        .placed_documents()
        .flat_map(|document| crate::resource_identity::placed_resources(&document, &execution))
        .collect::<Vec<_>>();
    for outer in [true, false] {
        for inner in [true, false] {
            let values = json!({"outer": outer, "inner": inner});
            let mut kinds = BTreeSet::new();
            for resource in &resources {
                if matches(&resource.condition, &values)? {
                    kinds.insert(resource.resource.kind.as_str());
                }
            }
            sim_assert_eq!(have: kinds, want: BTreeSet::from([
                if outer == inner { "ConfigMap" } else { "Secret" },
            ]));
        }
    }
    Ok(())
}

#[test]
fn shared_scalar_slots_retain_each_emission_source_span() {
    let source =
        "value: {{ if .Values.enabled }}{{ .Values.left }}{{ else }}{{ .Values.right }}{{ end }}";
    let path = Some("templates/probe.yaml");
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, path, &db);
    let expected = execution
        .sites()
        .filter(|site| site.rendered())
        .flat_map(|site| {
            site.value().inputs().into_iter().map(move |path| {
                (
                    path,
                    helm_schema_core::SourceSpan::new(site.span().start, site.span().end),
                )
            })
        })
        .collect::<BTreeSet<_>>();
    let projected = crate::tests::support::project_execution(source, path, &db, &execution);
    let contract = projected.contract.clone().finalize();
    let actual = contract
        .uses()
        .iter()
        .filter(|row| row.path.0 == ["value"])
        .flat_map(|row| {
            row.provenance
                .iter()
                .map(move |provenance| (row.source_expr.clone(), provenance.span))
        })
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(have: actual, want: expected);
}

#[test]
fn shared_scalar_choices_do_not_guard_unrelated_documents() {
    let source = indoc::indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        value: "{{ if .Values.enabled }}{{ .Values.left }}{{ else }}{{ .Values.right }}{{ end }}"
        ---
        apiVersion: v1
        kind: Secret
        data: {}
    "#};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, Some("templates/probe.yaml"), &db);
    let guards = execution
        .placed_documents()
        .flat_map(|document| crate::resource_identity::placed_resources(&document, &execution))
        .filter(|resource| resource.resource.kind == "Secret")
        .map(|resource| resource.condition)
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(have: guards, want: BTreeSet::from([Predicate::True]));
}

fn collection_matches(
    value: Option<&Value>,
    domain: CollectionDomain,
    quantifier: CollectionQuantifier,
    pattern: &str,
) -> eyre::Result<bool> {
    let pattern = regress::Regex::new(pattern)?;
    let selected = match domain {
        CollectionDomain::ObjectValues => value.and_then(Value::as_object).map(|items| {
            items
                .values()
                .map(|item| {
                    item.as_str()
                        .is_some_and(|text| pattern.find(text).is_some())
                })
                .collect::<Vec<_>>()
        }),
        CollectionDomain::ObjectKeys => value.and_then(Value::as_object).map(|items| {
            items
                .keys()
                .map(|text| pattern.find(text).is_some())
                .collect::<Vec<_>>()
        }),
    };
    Ok(selected.is_some_and(|items| match quantifier {
        CollectionQuantifier::All => items.into_iter().all(|matched| matched),
        CollectionQuantifier::Any => items.into_iter().any(|matched| matched),
    }))
}

/// A serialized contribution needs its container kind only while another member coexecutes.
#[test]
fn coexecuting_structural_contributions_keep_their_own_presence_domain() -> eyre::Result<()> {
    for sequence in [false, true] {
        let member = if sequence {
            "    - name: fixed\n"
        } else {
            "    fixed: true\n"
        };
        for before in [false, true] {
            let sibling = indoc::formatdoc! {r"
                {{{{- if .Values.enabled }}}}
                {member}{{{{- end }}}}
            "};
            let source = indoc::formatdoc! {r"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: witness
                spec:
                  slot:
                {before_text}{{{{ toYaml .Values.payload | nindent 4 }}}}
                {after_text}
            ", before_text = if before { sibling.as_str() } else { "" }, after_text = if before { "" } else { sibling.as_str() }};
            for enabled in [false, true] {
                for payload in [
                    Value::Null,
                    json!(false),
                    json!(0),
                    json!(""),
                    json!([]),
                    json!({}),
                    json!(["value"]),
                    json!({"key":"value"}),
                ] {
                    let valid = !enabled
                        || if sequence {
                            payload.is_array()
                                && payload.as_array().is_some_and(|items| !items.is_empty())
                        } else {
                            payload.is_object()
                                && payload.as_object().is_some_and(|fields| !fields.is_empty())
                        };
                    let values = json!({"enabled":enabled,"payload":payload});
                    sim_assert_eq!(have: rejection(&source, &values)?, want: !valid,
                        "sequence={sequence} before={before} values={values}");
                }
                sim_assert_eq!(have: rejection(&source, &json!({"enabled":enabled}))?, want: enabled,
                    "absent source: sequence={sequence} before={before}");
            }
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LiveOutputClaim {
    input: String,
    output: Vec<String>,
}

fn live_output_claims(source: &str, values: &Value) -> eyre::Result<BTreeSet<LiveOutputClaim>> {
    let defines = DefineIndex::new();
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    let mut claims = BTreeSet::new();
    for row in contract.uses().iter().filter(|row| !row.path.0.is_empty()) {
        let guard = Predicate::Or(
            row.condition
                .disjuncts()
                .iter()
                .map(|conjunction| Predicate::all(conjunction.iter().cloned().collect()))
                .collect(),
        );
        if matches(&guard, values)? {
            claims.insert(LiveOutputClaim {
                input: row.source_expr.encode(),
                output: row.path.0.clone(),
            });
        }
    }
    Ok(claims)
}

/// A local choice owns only its selected contribution and keeps the shared continuation.
#[test]
fn local_continuations_keep_every_selected_owner_and_guard() -> eyre::Result<()> {
    let source = indoc::indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: witness
        spec:
          {{- if .Values.a }}
          nodeSelector:
            {{- toYaml .Values.first | nindent 4 }}
          {{- else if .Values.b }}
          nodeSelector:
            {{- toYaml .Values.second | nindent 4 }}
          {{- else if .Values.c }}
          nodeSelector:
            {{- toYaml .Values.third | nindent 4 }}
          {{- end }}
          priorityClassName: {{ .Values.last }}
    "};
    for mask in 0..8 {
        let a = mask & 1 != 0;
        let b = mask & 2 != 0;
        let c = mask & 4 != 0;
        let values = json!({"a":a,"b":b,"c":c,"first":{"key":"one"},"second":{"key":"two"},"third":{"key":"three"},"last":"priority"});
        let mut want = BTreeSet::from([LiveOutputClaim {
            input: "last".to_owned(),
            output: vec!["spec".to_owned(), "priorityClassName".to_owned()],
        }]);
        if let Some(selected) = if a {
            Some("first")
        } else if b {
            Some("second")
        } else if c {
            Some("third")
        } else {
            None
        } {
            want.insert(LiveOutputClaim {
                input: selected.to_owned(),
                output: vec!["spec".to_owned(), "nodeSelector".to_owned()],
            });
        }
        sim_assert_eq!(have: live_output_claims(source, &values)?, want: want, "selection mask {mask}");
    }
    Ok(())
}

/// A later mapping member replaces the earlier value only under its own execution guard.
#[test]
fn ordered_mapping_contributions_withdraw_only_the_overwritten_occurrence() -> eyre::Result<()> {
    let source = indoc::indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: witness
        spec:
          nodeSelector:
            {{- toYaml .Values.before | nindent 4 }}
          {{- if .Values.replace }}
          nodeSelector:
            {{- toYaml .Values.after | nindent 4 }}
          {{- end }}
    "};
    for replace in [false, true] {
        let values = json!({"replace":replace,"before":{"key":"one"},"after":{"key":"two"}});
        let want = BTreeSet::from([LiveOutputClaim {
            input: if replace { "after" } else { "before" }.to_owned(),
            output: vec!["spec".to_owned(), "nodeSelector".to_owned()],
        }]);
        sim_assert_eq!(have: live_output_claims(source, &values)?, want: want);
    }
    Ok(())
}

/// A following plain sequence spelling can fold into the preceding serialized scalar.
#[test]
fn scalar_folding_remains_a_survivor_beside_a_sequence_spelling() -> eyre::Result<()> {
    let source = indoc::indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: witness
        spec:
          slot:
        {{ toYaml .Values.payload | nindent 4 }}
            - fixed
    "};
    for payload in [
        Value::Null,
        json!(false),
        json!(0),
        json!("word"),
        json!(["value"]),
    ] {
        sim_assert_eq!(have: rejection(source, &json!({"payload":payload}))?, want: false);
    }
    Ok(())
}
