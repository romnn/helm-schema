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
        PredicateKind::Guard(Guard::Range { path }) => match at(values, path) {
            Some(Value::Array(items)) => !items.is_empty(),
            Some(Value::Object(entries)) => !entries.is_empty(),
            None | Some(Value::Null) => false,
            Some(other) => eyre::bail!("unsupported range witness input {other}"),
        },
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
fn range_boundary_witness_keeps_the_real_suffix_in_its_probe() -> eyre::Result<()> {
    let source = indoc::indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          selector:
            matchLabels:
              app: test
          template:
            metadata:
              labels:
                app: test
            spec:
              {{- if .Values.probes }}
              containers:
              {{- range $_, $probe := .Values.probes }}
              - name: test
                image: busybox
                livenessProbe:
                  grpc:
                    service: {{ $probe.grpcService | quote }}
                    port: 8080
                readinessProbe:
                  httpGet:
                    path: {{ $probe.path | quote }}
              {{- end }}
                    port: {{ .Values.readinessPort }}
              {{- end }}
    "};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, Some("templates/probe.yaml"), &db);
    let port = ValuesPath::parse("readinessPort");
    let mut candidates = Vec::new();
    let memo = helm_schema_core::PredicateMemo::default();
    for document in execution.placed_documents() {
        for scalar in document.nodes() {
            if document.is_key(scalar)
                || !matches!(
                    document.kind(scalar),
                    Some(helm_schema_syntax::YamlNodeKind::Scalar(_))
                )
                || !document
                    .holes(scalar)
                    .iter()
                    .any(|term| term.inputs().contains(&port))
            {
                continue;
            }
            let ancestors =
                std::iter::successors(document.parent(scalar), |node| document.parent(*node));
            let mut path = ancestors
                .filter(|node| document.kind(*node) == Some(helm_schema_syntax::YamlNodeKind::Pair))
                .filter_map(|pair| {
                    document
                        .children(pair)
                        .iter()
                        .copied()
                        .find(|key| document.is_key(*key))
                        .and_then(|key| document.literal_key(key))
                })
                .collect::<Vec<_>>();
            path.reverse();
            let selected = Predicate::all(vec![
                document.condition.clone(),
                document.selection_guard(scalar),
            ]);
            candidates.push((path.join("."), selected));
            sim_assert_eq!(have: memo.normalize(document.placement_guard(scalar)), want: Predicate::False);
        }
    }
    // Test the structural owner on the selected arm. The opaque repetition still
    // prevents a placement claim after it.
    let probe = [
        json!({"path": "/first", "grpcService": "first"}),
        json!({"path": "/second", "grpcService": "second"}),
    ];
    for count in 0..=probe.len() {
        let values = json!({"probes": &probe[..count], "readinessPort": 8080});
        let mut paths = BTreeSet::new();
        for (path, selected) in &candidates {
            if matches(selected, &values)? {
                paths.insert(path.as_str());
            }
        }
        let expected = if count == 0 {
            BTreeSet::new()
        } else {
            BTreeSet::from(["spec.template.spec.containers.readinessProbe.httpGet.port"])
        };
        sim_assert_eq!(have: paths, want: expected, "probe count {count}");
    }
    Ok(())
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
          name: {{ .Values.name | trunc -63 }}
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
            // `trunc` survives only a string name; that check does not activate the resource
            // (ADR 0072 §2).
            activation: Some(Vec::new()),
        }
    ]));
    // A suffix of arbitrary text has no certified initial token state, so it
    // cannot establish placement of the later field.
    let placed = contract
        .uses()
        .iter()
        .filter(|use_| use_.resource.is_some())
        .map(|use_| (use_.source_expr.encode(), use_.path.0.clone(), use_.kind))
        .collect::<Vec<_>>();
    sim_assert_eq!(
        have: placed,
        want: vec![(
            "name".to_owned(),
            vec!["metadata".to_owned(), "name".to_owned()],
            crate::ValueKind::Serialized,
        )]
    );
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

/// An input-free constant that breaks its manifest still aborts under the guard that places it
/// (Helm 4.2.3, `transfer/abortfa-receipts/d1-minimal`: `{flag: true}` aborts with
/// `yaml: line 6: could not find expected ':'`; `{flag: false}` and `{}` render).
#[test]
fn a_constant_that_breaks_its_manifest_aborts_under_its_guard() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
        {{ if .Values.flag }}{{ "d" | toYaml }}{{ end }}
    "#};
    for (values, aborts) in [
        (json!({"flag": true}), true),
        (json!({"flag": false}), false),
        (json!({}), false),
    ] {
        sim_assert_eq!(have: rejection(source, &values)?, want: aborts, "{values}");
    }
    Ok(())
}

/// The constant arm of a serialized payload is judged in its completed manifest
/// (Helm 4.2.3, `transfer/abortfa-receipts/d1-minimal/default.docs`): a falsy `other` selects
/// the bare scalar `d` at the document root, which aborts; a non-empty map renders.
#[test]
fn a_serialized_constant_arm_that_breaks_its_manifest_aborts_under_its_guard() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
        {{ if .Values.flag }}{{ default "d" .Values.other | toYaml }}{{ end }}
    "#};
    for (values, aborts) in [
        (json!({"flag": true}), true),
        (json!({"flag": true, "other": null}), true),
        (json!({"flag": true, "other": {}}), true),
        (json!({"flag": true, "other": "x"}), true),
        (json!({"flag": true, "other": [1]}), true),
        (json!({"flag": true, "other": {"c": "d"}}), false),
        (json!({"flag": false}), false),
    ] {
        sim_assert_eq!(have: rejection(source, &values)?, want: aborts, "{values}");
    }
    Ok(())
}

/// Helm decodes the completed mapping, where a later duplicate metadata key replaces the list.
#[test]
fn a_serialized_constant_arm_uses_the_completed_manifest_head() -> eyre::Result<()> {
    let source = indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ default (list 1) .Values.other | toYaml | nindent 4 }}
        metadata: {name: probe}
    "};
    for source in [
        source.to_owned(),
        source.replace("(list 1)", "(dict \"x\" 1)"),
        source.replace("metadata: {name: probe}", "  name: probe"),
        source
            .replace(
                "name: {{ default (list 1)",
                "annotations: {{ default \"scalar\"",
            )
            .replace(" | nindent 4", ""),
    ] {
        for values in [json!({}), json!({"other": null}), json!({"other": {}})] {
            sim_assert_eq!(have: rejection(&source, &values)?, want: false, "{source}: {values}");
        }
    }
    Ok(())
}

/// sigs.k8s.io/yaml coerces numbers to the string field in Helm's manifest head.
#[test]
fn a_serialized_numeric_constant_name_renders() -> eyre::Result<()> {
    let source = indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ default 1 .Values.other | toYaml }}
    "};
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);
    Ok(())
}

/// sigs.k8s.io/yaml coerces booleans to the string field in Helm's manifest head.
#[test]
fn a_serialized_boolean_constant_kind_renders() -> eyre::Result<()> {
    let source = indoc::indoc! {"
        apiVersion: v1
        kind: {{ default true .Values.other | toYaml }}
        metadata:
          name: probe
    "};
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);
    Ok(())
}

/// Helm's manifest head ignores labels, even when their value is a scalar.
#[test]
fn a_serialized_scalar_constant_labels_renders() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
          labels: {{ default "scalar" .Values.other | toYaml }}
    "#};
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);
    Ok(())
}

#[test]
fn a_serialized_constant_keeps_inherited_capture_indentation() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        {{ define "body" -}}
        name: probe
        extra: {{ default (list 1) .Values.other | toYaml | nindent 2 }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:{{ include "body" . | nindent 2 }}
    "#};
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);
    Ok(())
}

/// Distinct YAML keys can become one JSON key in either Go map iteration order.
#[test]
fn a_serialized_constant_does_not_decide_colliding_json_keys() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
          annotations:
            true: ok
            "true": {{ default (list 1) .Values.other | toYaml | nindent 6 }}
    "#};
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);
    Ok(())
}

/// Helm's preceding separator already consumed the newline before this ordinary YAML key.
#[test]
fn a_serialized_constant_keeps_the_original_split_context() -> eyre::Result<()> {
    let source = indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata: {name: probe}
        ---
        ---kind:{{ default (list 1) .Values.other | toYaml | nindent 2 }}
    "};
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);
    let with_prefix = source.replace("---\n", "note: {{ .Values.prefix }}\n");
    sim_assert_eq!(have: rejection(&with_prefix, &json!({"prefix": "ok\n---"}))?, want: false);
    Ok(())
}

/// A missing prefix lets Helm's first separator consume the next separator's newline.
#[test]
fn refused_chunks_require_rendered_document_boundaries() -> eyre::Result<()> {
    let source = indoc::indoc! {"
        ---
        {{ .Values.prefix }}
        ---kind: [1]
        metadata:
          <<: {name: probe}
    "};
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);

    // With a concrete nonempty prefix, Helm splits again and decodes the invalid kind.
    let nonempty = source.replace("{{ .Values.prefix }}", "prefix: ok");
    sim_assert_eq!(have: rejection(&nonempty, &json!({}))?, want: true);
    Ok(())
}

/// A merge key sends this exact chunk through the fallback head decoder.
#[test]
fn refused_chunks_leave_colliding_json_keys_undecided() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          <<: {name: probe}
          annotations: {true: ok, "true": [1]}
    "#};
    sim_assert_eq!(have: rejection(source, &json!({}))?, want: false);
    Ok(())
}

/// Whether `values` place a Scalar row for `path` in the executed document.
fn placed(source: &str, path: &[&str], values: &Value) -> eyre::Result<bool> {
    let idx = DefineIndex::new();
    let ir = crate::SymbolicIrContext::new(&idx)
        .generate_contract_ir(source)
        .finalize();
    let path = crate::YamlPath(path.iter().map(|segment| (*segment).to_owned()).collect());
    for use_ in ir
        .uses()
        .iter()
        .filter(|use_| use_.path == path && use_.kind == crate::ValueKind::Scalar)
    {
        for disjunct in use_.condition.disjuncts() {
            let mut holds = true;
            for predicate in disjunct {
                holds &= matches(predicate, values)?;
            }
            if holds {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `: ` inside a block value opens a mapping when the value starts on a later line; Helm 4.2.3
/// refuses it only on the key's own line (transfer/helm-receipts/dash-plain-start/receipts.txt,
/// review r2).
#[test]
fn a_colon_space_value_fails_only_on_its_keys_line() -> eyre::Result<()> {
    let inline = indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: inline
        data: {{ .Values.payload }}
    "};
    let mut rejected_next_line = Vec::new();
    for payload in ["-a: b", "a: b"] {
        let values = json!({"payload": payload});
        sim_assert_eq!(have: rejection(inline, &values)?, want: true, "inline {payload:?}");
        for line_break in ["\n", "\r\n", "\r", "\u{85}", "\u{2028}", "\u{2029}"] {
            let next_line = inline.replace("data: ", &format!("data:{line_break}  "));
            if rejection(&next_line, &values)? {
                rejected_next_line.push((line_break, payload));
            }
        }
    }
    sim_assert_eq!(have: rejected_next_line.as_slice(), want: &[]);
    Ok(())
}

/// Brackets end a plain scalar inside a flow collection, so Helm 4.2.3 cannot parse
/// `[--x=a[b]]` while the block spelling is plain text (receipts, review r2); a later field is
/// placed only for flow-safe spellings.
#[test]
fn flow_plain_tokens_do_not_absorb_brackets() -> eyre::Result<()> {
    let source = indoc::indoc! {"
        apiVersion: v1
        kind: Pod
        metadata:
          name: flow
        spec:
          containers:
            - name: c
              args: [--x={{ .Values.x }}]
              image: {{ .Values.image }}
    "};
    let image = ["spec", "containers[*]", "image"];
    for (x, want) in [("a", true), ("a[b]", false)] {
        sim_assert_eq!(
            have: placed(source, &image, &json!({"x": x, "image": "nginx"}))?,
            want: want,
            "x={x:?}"
        );
    }
    Ok(())
}

#[test]
fn a_selected_separator_detaches_the_shared_metadata_from_the_configmap() -> eyre::Result<()> {
    let source = indoc::indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        {{- if .Values.split }}
        data: {}
        ---
        {{- else }}
        data: {}
        {{- end }}
        metadata:
          namespace: {{ .Values.namespaceOverride }}
    "};
    assert_configmap_namespace_paths(
        source,
        &[
            (json!({"split": true, "namespaceOverride": true}), 0),
            (json!({"split": false, "namespaceOverride": true}), 1),
        ],
    )
}

#[test]
fn widening_later_mapping_choices_preserves_established_namespace_placement() -> eyre::Result<()> {
    let source = indoc::indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
          namespace: {{ .Values.namespaceOverride }}
        data:
          {{- if .Values.a }}
          a: fixed
          {{- end }}
          {{- if .Values.b }}
          b: fixed
          {{- end }}
          {{- if .Values.c }}
          c: fixed
          {{- end }}
          {{- if .Values.d }}
          d: fixed
          {{- end }}
          {{- if .Values.e }}
          e: fixed
          {{- end }}
          {{- if .Values.f }}
          f: fixed
          {{- end }}
          {{ .Values.key | quote }}: fixed
    "};
    let cases = (0..64)
        .map(|mask| {
            let mut values = ["a", "b", "c", "d", "e", "f"]
                .into_iter()
                .enumerate()
                .map(|(bit, key)| (key.to_owned(), json!(mask & (1 << bit) != 0)))
                .collect::<serde_json::Map<_, _>>();
            values.insert("namespaceOverride".to_owned(), json!(true));
            values.insert("key".to_owned(), json!("tail"));
            (Value::Object(values), 1)
        })
        .collect::<Vec<_>>();
    assert_configmap_namespace_paths(source, &cases)?;
    let immutable = ["immutable: false", ""].join("\n");
    let later_document = indoc::indoc! {"
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: later
          namespace: ops
    "};
    for tail in [&immutable, later_document] {
        assert_configmap_namespace_paths(&[source, tail].concat(), &cases)?;
    }
    let shadow = indoc::indoc! {r#"
        {{- if .Values.shadow }}
        "\u006detadata":
          name: test
          namespace: ops
        {{- else }}
        immutable: false
        {{- end }}
    "#};
    let mut shadow_cases = Vec::new();
    for (values, _) in cases {
        if values.get("a") != Some(&Value::Bool(false)) {
            continue;
        }
        for shadow in [true, false] {
            let mut configured = values.clone();
            if let Some(configured) = configured.as_object_mut() {
                configured.insert("shadow".to_owned(), json!(shadow));
            }
            shadow_cases.push((configured, usize::from(!shadow)));
        }
    }
    let shadowed = [source, shadow].concat();
    let namespace = LiveOutputClaim {
        input: "namespaceOverride".to_owned(),
        output: vec!["metadata".to_owned(), "namespace".to_owned()],
    };
    for (values, expected) in shadow_cases {
        let placed = live_output_claims(&shadowed, &values)?.contains(&namespace);
        sim_assert_eq!(have: usize::from(placed), want: expected, "values={values}");
    }
    Ok(())
}

fn assert_configmap_namespace_paths(source: &str, cases: &[(Value, usize)]) -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, Some("templates/probe.yaml"), &db);
    for (values, expected) in cases {
        let mut placed = false;
        for document in execution.placed_documents() {
            if !matches(&document.condition, values)?
                || !crate::resource_identity::placed_resources(&document, &execution)
                    .into_iter()
                    .any(|resource| resource.resource.kind == "ConfigMap")
            {
                continue;
            }
            for node in document.nodes() {
                if document.is_key(node)
                    && document.literal_key(node) == Some("namespace")
                    && matches(&document.placement_guard(node), values)?
                    && let Some(pair) = document.parent(node)
                {
                    for &value in document.children(pair) {
                        if !document.is_key(value)
                            && !document.holes(value).is_empty()
                            && matches(&document.placement_guard(value), values)?
                        {
                            placed = true;
                        }
                    }
                }
            }
        }
        sim_assert_eq!(have: usize::from(placed), want: *expected, "values={values}");
    }
    Ok(())
}

fn later_mapping_choices(width: usize) -> eyre::Result<String> {
    let indent = " ".repeat(width);
    let mut tail = String::new();
    for key in ['a', 'b', 'c', 'd', 'e', 'f'] {
        writeln!(tail, "{indent}{{{{- if .Values.{key} }}}}")?;
        writeln!(tail, "{indent}{key}: fixed")?;
        writeln!(tail, "{indent}{{{{- end }}}}")?;
    }
    writeln!(tail, "{indent}{{{{ .Values.key | quote }}}}: fixed")?;
    Ok(tail)
}

#[test]
fn widening_a_child_walk_keeps_its_manifest_head_error() -> eyre::Result<()> {
    let start = indoc::indoc! {"
        {{- if .Values.create }}
        apiVersion: storage.k8s.io/v1
        kind: StorageClass
        metadata:
          name: probe
          annotations: {{- toYaml .Values.annotations | nindent 4 }}
        provisioner: example.com/provisioner
        parameters:
    "};
    let source = [start, &later_mapping_choices(2)?, "{{- end }}\n"].concat();
    for (create, annotations, aborts) in [
        (true, json!(1.5), true),
        (true, json!({"a": "b"}), false),
        (false, json!(1.5), false),
    ] {
        let values = json!({"create": create, "annotations": annotations, "key": "tail"});
        sim_assert_eq!(have: rejection(&source, &values)?, want: aborts, "values={values}");
    }
    Ok(())
}

#[test]
fn widening_an_open_mapping_keeps_a_closed_sequence_item_placement() -> eyre::Result<()> {
    let start = indoc::indoc! {"
        apiVersion: batch/v1
        kind: Job
        metadata:
          name: probe
        spec:
          template:
            spec:
              restartPolicy: Never
              containers:
              - name: selected
                image: image:tag
                imagePullPolicy: {{ .Values.policy }}
              - name: other
                image: image:tag
                imagePullPolicy: Always
              nodeSelector:
    "};
    let source = [start, &later_mapping_choices(8)?].concat();
    let path = [
        "spec",
        "template",
        "spec",
        "containers[*]",
        "imagePullPolicy",
    ];
    let values = json!({"policy": [], "key": "tail"});
    sim_assert_eq!(have: placed(&source, &path, &values)?, want: true);
    Ok(())
}

/// A byte prefix of a plain composed word keeps the replicas slot. A name
/// that emits another document may move the slot, as in the Helm 4.2.3
/// receipts under `transfer/r4/trunc-word`.
#[test]
fn truncating_a_composed_word_keeps_the_following_plain_document_slot() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: {{ printf "%s-frontend" (.Values.name | trimSuffix "-") | trunc (sub 104 (len "frontend") | int) }}
        spec:
          replicas: {{ .Values.replicas }}
    "#};
    sim_assert_eq!(
        have: placed(source, &["spec", "replicas"], &json!({"name": "ordinary-probe", "replicas": "three"}))?,
        want: true
    );
    let moved = indoc::indoc! {"
        split
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: moved
    "};
    sim_assert_eq!(
        have: placed(source, &["spec", "replicas"], &json!({"name": moved.trim_end(), "replicas": "three"}))?,
        want: false
    );
    Ok(())
}
