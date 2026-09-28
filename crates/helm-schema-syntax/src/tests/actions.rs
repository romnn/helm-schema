use test_util::prelude::sim_assert_eq;

use crate::{ActionKind, TemplatedDocument};

/// Each action keeps its own delimiter kinds: `{{-3}}` opens with a plain
/// `{{` before the literal `-3`, unlike the trimming `{{- 3}}`.
#[test]
fn actions_retain_both_delimiter_kinds() {
    let source = concat!(
        "a: {{- .x -}} {{-3}} {{- 3}}\n",
        "b: {{ template \"t\" . -}}{{- break }}\n",
        "{{- if .y }}z{{- else -}}w{{ end -}}\n",
    );
    let document = TemplatedDocument::parse(source);
    let have: Vec<(&str, bool, bool)> = document
        .actions()
        .iter()
        .map(|action| {
            (
                source.get(action.span.start..action.span.end).unwrap_or(""),
                action.trim_left,
                action.trim_right,
            )
        })
        .collect();
    sim_assert_eq!(
        have: have,
        want: vec![
            ("{{- .x -}}", true, true),
            ("{{-3}}", false, false),
            ("{{- 3}}", true, false),
            ("{{ template \"t\" . -}}", false, true),
            ("{{- break }}", true, false),
            ("{{- if .y }}", true, false),
            ("{{- else -}}", true, true),
            ("{{ end -}}", false, true),
        ]
    );
}

/// Control brackets keep their region roles, and output actions keep the
/// expression span between their delimiters.
#[test]
fn actions_retain_kinds_in_byte_order() {
    let source = "{{if(.Values.flag)}}{{ .Values.x }}{{else}}{{/* c */}}{{end}}";
    let document = TemplatedDocument::parse(source);
    let have: Vec<(&str, &str)> = document
        .actions()
        .iter()
        .map(|action| {
            let kind = match action.kind {
                ActionKind::Output { expr_span, .. } => {
                    source.get(expr_span.start..expr_span.end).unwrap_or("")
                }
                ActionKind::RegionOpen { .. } => "open",
                ActionKind::RegionBranch { .. } => "branch",
                ActionKind::RegionEnd { .. } => "end",
                ActionKind::TemplateComment => "comment",
                ActionKind::Assign
                | ActionKind::Break
                | ActionKind::Continue
                | ActionKind::Error => "other",
            };
            (
                source.get(action.span.start..action.span.end).unwrap_or(""),
                kind,
            )
        })
        .collect();
    sim_assert_eq!(
        have: have,
        want: vec![
            ("{{if(.Values.flag)}}", "open"),
            ("{{ .Values.x }}", ".Values.x"),
            ("{{else}}", "branch"),
            ("{{/* c */}}", "comment"),
            ("{{end}}", "end"),
        ]
    );
}
