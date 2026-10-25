use crate::analysis_db::IrAnalysisDb;
use crate::value::{DocumentExecution, execute_document};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

fn execute(source: &str) -> DocumentExecution {
    execute_document(source, None, &IrAnalysisDb::new(&DefineIndex::new()))
}

#[test]
fn constant_semver_comparison_controls_reachability() {
    for (constraint, expected) in [("<1.6-0", false), ("^1.6-0", true)] {
        let source = format!(
            "{{{{ if semverCompare {constraint:?} \"v1.35.0\" }}}}yes{{{{ else }}}}no{{{{ end }}}}"
        );
        let result = execute(&source);
        sim_assert_eq!(have: result.abort_predicate(), want: Predicate::False);
        sim_assert_eq!(
            have: result.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(),
            want: vec![Some(if expected { "yes" } else { "no" }.to_owned())]
        );
    }
}

#[test]
fn required_selected_subject_keeps_each_consumed_empty_arm() {
    let execution = execute(r#"{{ required "subject" (ternary "" "yes" .Values.empty) }}"#);
    let boolean = Predicate::from(Guard::TypeIs {
        path: ValuesPath::parse("empty"),
        schema_type: "boolean".to_owned(),
    });
    let truth = Predicate::from(Guard::Truthy {
        path: ValuesPath::parse("empty"),
    });
    let aborted = execution.abort_predicate();
    assert!(boolean.negated().exactly_implies(&aborted));
    assert!(
        Predicate::all(vec![boolean.clone(), truth.clone()]).exactly_implies(&aborted),
        "{aborted:?}"
    );
    assert!(
        Predicate::all(vec![boolean, truth.negated()]).exactly_implies(&aborted.negated()),
        "{aborted:?}"
    );
}

#[test]
fn mixed_selected_operands_retain_the_rejected_arm() {
    let boolean = Predicate::from(Guard::TypeIs {
        path: ValuesPath::parse("choose"),
        schema_type: "boolean".to_owned(),
    });
    let truth = Predicate::from(Guard::Truthy {
        path: ValuesPath::parse("choose"),
    });
    let expected = Predicate::Or(vec![boolean.negated(), truth.negated()]).normalize_boolean();
    for source in [
        r#"{{ required "subject" (ternary (dict) "" .Values.choose) }}"#,
        r#"{{ required "subject" (ternary (list) nil .Values.choose) }}"#,
        r#"{{ ternary "a" "b" (ternary false 0 .Values.choose) }}"#,
    ] {
        let execution = execute(source);
        sim_assert_eq!(
            have: execution.abort_predicate(),
            want: expected.clone(),
            "{source}"
        );
    }
}

#[test]
fn negation_keeps_the_frozen_input_condition() {
    let execution = execute(r#"{{ if not (eq .Values.mode "ok") }}{{ fail "mode" }}{{ end }}"#);
    let expected = Predicate::from(Guard::Eq {
        path: ValuesPath::parse("mode"),
        value: GuardValue::String("ok".to_owned()),
    })
    .negated();
    let equality = expected.negated();
    let equality_has_string_type = Predicate::Or(vec![
        equality.negated(),
        Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("mode"),
            schema_type: "string".to_owned(),
        }),
    ]);
    let observed = Predicate::all(vec![
        execution.abort_predicate(),
        equality_has_string_type.clone(),
    ]);
    let expected = Predicate::all(vec![expected, equality_has_string_type]);
    assert!(observed.exactly_implies(&expected));
    assert!(expected.exactly_implies(&observed));
}

#[test]
fn direct_fractional_literal_cannot_become_an_integer_parameter() {
    sim_assert_eq!(have: execute(r#"{{ repeat 1.5 "x" }}"#).abort_predicate(), want: Predicate::True);
    sim_assert_eq!(have: execute(r#"{{ repeat (1.0) "x" }}"#).abort_predicate(), want: Predicate::True);
}

#[test]
fn signature_failure_precedes_later_argument_effects() {
    let execution =
        execute(r#"{{ $d := dict "x" "old" }}{{ ternary (upper 3) (set $d "x" "new") true }}"#);
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::True);
    sim_assert_eq!(have: execution.invocations().filter(|record| record.function() == "set").count(), want: 0);
}

#[test]
fn exact_runtime_type_predicates_do_not_invalidate_aliases() {
    let execution = execute(
        r#"{{ $d := dict "x" "kept" }}{{ kindIs "string" $d.x }}/{{ typeIs "map[string]interface {}" $d }}/{{ $d.x }}"#,
    );
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("true/true/kept".to_owned())]);
}

#[test]
fn tpl_strips_the_complete_child_buffer_before_comparisons() {
    for source in [
        r#"{{ if ne (tpl "<no value>" .) "" }}{{ fail "unstripped" }}{{ end }}ok"#,
        r#"{{ if ne (tpl `{{ print "<no" }} value>` .) "" }}{{ fail "split marker" }}{{ end }}ok"#,
        r#"{{ if ne (tpl `<no {{ print "value>" }}` .) "" }}{{ fail "split marker" }}{{ end }}ok"#,
    ] {
        let execution = execute(source);
        sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("ok".to_owned())]);
    }
}

#[test]
fn include_preserves_the_marker_at_the_return_boundary() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "templates/_helpers.tpl",
        r#"{{ define "marker" }}<no value>{{ end }}"#,
    );
    let execution = execute_document(
        r#"{{ len (include "marker" .) }}/{{ len (tpl "<no value>" .) }}"#,
        None,
        &IrAnalysisDb::new(&defines),
    );
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("10/0".to_owned())]);
}

#[test]
fn symbolic_tpl_replacement_cannot_support_an_exact_abort() {
    let execution = execute(
        r#"{{ if ne (tpl `<no {{ .Values.part }}` .) "" }}{{ fail "possible marker" }}{{ end }}"#,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
}

#[test]
fn nil_is_converted_only_by_a_concrete_parameter() {
    for source in [
        r#"{{ required "subject" (unset nil "x") }}"#,
        r#"{{ required "subject" (merge nil) }}"#,
    ] {
        sim_assert_eq!(have: execute(source).abort_predicate(), want: Predicate::False);
    }
    for source in [r#"{{ required "subject" nil }}"#, r#"{{ set nil "x" 1 }}"#] {
        sim_assert_eq!(have: execute(source).abort_predicate(), want: Predicate::True);
    }
}

#[test]
fn comparisons_preserve_typed_nil_and_stop_after_a_match() {
    let execution = execute(
        r#"{{ eq (fromJson "null") nil }}/{{ eq (fromJsonArray "null") nil }}/{{ eq (dict) nil }}/{{ eq (int64 1) 1 }}/{{ eq 1 1 (dict) }}"#,
    );
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("true/true/false/true/true".to_owned())]);
    for source in [
        r#"{{ eq (float64 1) 1 }}"#,
        r#"{{ eq (dict) (dict) }}"#,
        r#"{{ eq 1 2 (dict) }}"#,
        r#"{{ eq 1 }}"#,
    ] {
        sim_assert_eq!(have: execute(source).abort_predicate(), want: Predicate::True);
    }
}

#[test]
fn all_map_alternatives_fail_a_string_parameter_even_with_unknown_nilness() {
    sim_assert_eq!(have: execute(r#"{{ upper (fromJson .Values.document) }}"#).abort_predicate(), want: Predicate::True);
}

#[test]
fn direct_map_lookup_nil_fails_before_parameter_conversion() {
    for source in [
        r#"{{ $d := dict }}{{ keys $d.m }}"#,
        r#"{{ $d := dict "m" nil }}{{ keys $d.m }}"#,
        r#"{{ keys .Values.m }}"#,
    ] {
        let expected = if source.contains(".Values") {
            Predicate::from(Guard::TypeIs {
                path: ValuesPath::parse("m"),
                schema_type: "object".to_owned(),
            })
            .negated()
        } else {
            Predicate::True
        };
        sim_assert_eq!(have: execute(source).abort_predicate(), want: expected);
    }
    for source in [
        r#"{{ $d := dict }}{{ keys ($d.m) }}"#,
        r#"{{ $d := dict }}{{ $m := $d.m }}{{ keys $m }}"#,
        r#"{{ $d := dict }}{{ $d.m | keys }}"#,
        r#"{{ $d := dict "m" (fromJson "null") }}{{ keys $d.m }}"#,
    ] {
        sim_assert_eq!(have: execute(source).abort_predicate(), want: Predicate::False);
    }
}

#[test]
fn float_zero_equality_uses_numeric_value() {
    for source in [
        r#"{{ if ne -0.0 0.0 }}{{ fail "zero must compare equal" }}{{ end }}ok"#,
        r#"{{ if ne (float64 false) -0.0 }}{{ fail "zero must compare equal" }}{{ end }}ok"#,
    ] {
        let execution = execute(source);
        sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("ok".to_owned())]);
    }
}

#[test]
fn values_yaml_method_is_not_an_input_key() {
    let execution = execute(r#"{{ upper .Values.YAML }}"#);
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
}

#[test]
fn nil_helper_receiver_does_not_acquire_a_boxed_nil_lookup_failure() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "templates/_helpers.tpl",
        r#"{{ define "h" }}{{ keys .m | toJson }}{{ end }}"#,
    );
    let execution = execute_document(
        r#"{{ include "h" nil }}"#,
        None,
        &IrAnalysisDb::new(&defines),
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
}

#[test]
fn unresolved_go_string_bytes_keep_type_without_inventing_content() {
    let execution = execute(r#"{{ if ne "\xff" "not the byte" }}yes{{ end }}"#);
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    let typed = execute(r#"{{ typeIs "string" "\xff" }}"#);
    sim_assert_eq!(have: typed.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("true".to_owned())]);
    for source in [
        r#"{{ if ne "\303\251" "é" }}{{ fail "octal" }}{{ end }}ok"#,
        r#"{{ if ne "\xc3\xa9" "é" }}{{ fail "hex" }}{{ end }}ok"#,
    ] {
        sim_assert_eq!(have: execute(source).abort_predicate(), want: Predicate::False);
    }
}
