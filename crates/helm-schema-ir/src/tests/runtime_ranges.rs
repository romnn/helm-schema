use crate::analysis_db::IrAnalysisDb;
use crate::value::{AbortReason, Flow, State, Value, eval::Executor, execute_document};
use helm_schema_ast::DefineIndex;
use helm_schema_core::Predicate;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn text(source: &str) -> Vec<Option<String>> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, None, &db);
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    execution
        .text()
        .map(|(_, value)| value.exact_text())
        .collect()
}

#[test]
fn integer_range_preserves_nonpositive_zero_and_one_variable_semantics() {
    for (source, expected) in [
        (r"{{ range -1 }}wrong{{ else }}empty{{ end }}", "empty"),
        (r"{{ range $i := 0 }}wrong{{ else }}empty{{ end }}", "empty"),
        (r"{{ range 3 }}{{ . }}{{ end }}", "012"),
        (r"{{ range $i := int64 3 }}{{ $i }}{{ end }}", "012"),
        (
            r"{{ range 5 }}{{ if eq . 1 }}{{ continue }}{{ end }}{{ if eq . 3 }}{{ break }}{{ end }}{{ . }}{{ end }}",
            "02",
        ),
        (
            r"{{ range (fromJson `null`) }}wrong{{ else }}empty{{ end }}",
            "empty",
        ),
    ] {
        sim_assert_eq!(have: text(source), want: vec![Some(expected.to_owned())], "{source}");
    }
}

#[test]
fn invalid_range_types_and_two_integer_variables_abort_before_the_body() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r"{{ range $i, $v := 0 }}{{ end }}",
        r"{{ range $i, $v := int64 3 }}{{ end }}",
        r#"{{ range (get (fromJson `{"n":3}`) "n") }}{{ end }}"#,
        r"{{ range false }}{{ else }}wrong{{ end }}",
        r#"{{ range "" }}{{ else }}wrong{{ end }}"#,
    ] {
        sim_assert_eq!(have: execute_document(source, None, &db).abort_predicate(), want: Predicate::True, "{source}");
    }
}

#[test]
fn string_range_keeps_current_and_later_backing_slots_live() {
    // Go's range variables retain reflect-backed string slots, including the current element.
    let source = indoc! {r#"
        {{- $xs := splitList "," "b,a" -}}
        {{- range $i, $v := $xs -}}
          {{- if eq $i 0 -}}{{- $_ := sortAlpha $xs -}}{{- end -}}
          {{- $v -}}/{{- . -}};
        {{- end -}}
    "#};
    sim_assert_eq!(have: text(source), want: vec![Some("a/a;b/b;".to_owned())]);
}

#[test]
fn map_range_captures_entries_but_keeps_nested_map_references_live() {
    // Replacing a map entry does not replace its captured value; nested maps remain aliased.
    let source = indoc! {r#"
        {{- $nested := dict "x" "old" -}}
        {{- $d := dict "a" "old" "b" "old" "c" $nested -}}
        {{- range $key, $value := $d -}}
          {{- if eq $key "a" -}}
            {{- $_ := set $d "b" "new" -}}
            {{- $_ := set $nested "x" "new" -}}
          {{- end -}}
          {{- if eq $key "c" -}}{{- $value.x -}}{{- else -}}{{- $value -}}{{- end -}};
        {{- end -}}
    "#};
    sim_assert_eq!(have: text(source), want: vec![Some("old;old;new;".to_owned())]);
}

#[test]
fn nonempty_over_budget_ranges_preserve_abort_and_do_not_execute_else() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for count in [31, 32, 33] {
        let members = (0..count)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        for source in [
            format!(
                r#"{{{{ range list {members} }}}}{{{{ fail "body" }}}}{{{{ else }}}}wrong{{{{ end }}}}"#
            ),
            format!(
                r#"{{{{ range {count} }}}}{{{{ fail "body" }}}}{{{{ else }}}}wrong{{{{ end }}}}"#
            ),
        ] {
            sim_assert_eq!(have: execute_document(&source, None, &db).abort_predicate(), want: Predicate::True, "{source}");
        }
    }
}

#[test]
fn unknown_range_keeps_zero_exit_and_terminal_body_flow() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let exits = Executor { db: &db }.body(
        r#"{{ range . }}{{ fail "body" }}{{ else }}zero{{ end }}"#,
        None,
        State::new(Value::unknown()),
    );
    sim_assert_eq!(have: exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
    sim_assert_eq!(have: exits.iter().filter(|exit| matches!(exit.flow, Flow::Abort(AbortReason::Fail))).count(), want: 1);
}

#[test]
fn range_precision_does_not_replace_integer_or_slice_runtime_types() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r#"{{ range int64 3 }}{{ if not (typeIs "int64" .) }}{{ fail "integer type" }}{{ end }}{{ end }}"#,
        r#"{{ if typeIs "[]interface {}" (until 3) }}{{ fail "numeric slice type" }}{{ end }}"#,
        r#"{{ if eq (toJson (get (fromJson `{"n":3}`) "n")) "3.0" }}{{ fail "numeric serialization" }}{{ end }}"#,
    ] {
        sim_assert_eq!(have: execute_document(source, None, &db).abort_predicate(), want: Predicate::False, "{source}");
    }
}
