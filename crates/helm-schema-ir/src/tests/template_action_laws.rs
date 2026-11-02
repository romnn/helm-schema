use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, Predicate, PredicateKind, ValuePattern, ValuesPath};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use super::execute_document;
use crate::analysis_db::IrAnalysisDb;

#[test]
fn an_opaque_choice_does_not_supply_template_action_certificates() {
    use super::super::{Coverage, State, Value};
    let state = State::new(Value::nil());
    let choice = state
        .heap
        .arena
        .decision(Predicate::True, Predicate::True, Coverage::Partial);
    let source = Value::select(choice, Value::string("safe"), Value::string("{{"));
    let result = super::super::text_actions::contains(&source, "{{", &state);
    let test = state.test(&result);
    sim_assert_eq!(have: test.coverage, want: Coverage::Partial);
    sim_assert_eq!(have: test.when_true.clone(), want: Predicate::False);
    sim_assert_eq!(have: test.when_false.clone(), want: Predicate::False);
}

#[test]
fn release_fields_have_helm_types_without_values_aliases() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for (field, kind) in [
        ("Name", "String"),
        ("Namespace", "String"),
        ("Service", "String"),
        ("IsInstall", "Bool"),
        ("IsUpgrade", "Bool"),
        ("Revision", "Int"),
    ] {
        let source = format!("{{{{ .Release.{field} }}}}");
        let execution = execute_initial_release(&source, &db);
        let output = execution
            .sites()
            .last()
            .ok_or_eyre("missing host observation")?;
        sim_assert_eq!(have: format!("{:?}", output.value().go_type()), want: kind);
        sim_assert_eq!(have: output.value().inputs(), want: BTreeSet::default());
    }
    Ok(())
}

#[test]
fn validated_release_name_cannot_supply_a_template_opener() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r#"{{ contains "{{" .Release.Name }}"#,
        r#"{{ contains "{{" (quote .Release.Name .Release.Service) }}"#,
        r#"{{ contains "{{" (quote nil .Release.Name nil .Release.Service nil) }}"#,
        r#"{{ contains "{{" (toJson (dict "release" .Release.Name "service" .Release.Service)) }}"#,
        r#"{{ contains "{{" (toJson (dict "release" .Release.Name "install" .Release.IsInstall "upgrade" .Release.IsUpgrade "revision" .Release.Revision)) }}"#,
        r#"{{ contains "{{" (printf "%s-%s" (.Release.Name | trunc 10 | trimSuffix "-") .Release.Service) }}"#,
    ] {
        let execution = execute_initial_release(source, &db);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("false".to_owned())]);
    }
}

#[test]
fn variadic_quotes_omit_nil_operands_and_only_separate_emitted_operands() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for (source, expected) in [
        (r#"{{ quote nil "one" nil "two" nil }}"#, r#""one" "two""#),
        (r"{{ quote nil nil }}", ""),
        (r#"{{ quote "" nil "" }}"#, r#""" """#),
    ] {
        let execution = execute_document(source, None, &db);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some(expected.to_owned())]);
    }
}

#[test]
fn text_composition_preserves_literal_and_cross_boundary_openers() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for (expression, expected) in [
        (r#"printf "%s%s" "{" "{""#, true),
        (r#"print "{" "" "{""#, true),
        (r#"printf "%s%s" "{" (toJson (dict))"#, true),
        (r#"quote (printf "%s%s" "{" "{")"#, true),
        (r#"quote "{{🙂""#, true),
        (r#"quote "🙂plain""#, false),
        (r#"toJson (list (dict "{{key" .Release.Name))"#, true),
        (r#"replace "" "-" "{{""#, false),
        (r#"replace "x" "" "{x{""#, true),
        (r#"trunc 2 "a{{""#, false),
        (r#"trunc 1 "{{""#, false),
        (r#"trunc -2 "a{{""#, true),
        (r#"trimSuffix "{{" "a{{""#, false),
        (r#"trimPrefix "{{" "{{a""#, false),
    ] {
        let source = format!(r#"{{{{ contains "{{{{" ({expression}) }}}}"#);
        let execution = execute_initial_release(&source, &db);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some(expected.to_string())], "{expression}");
    }
}

#[test]
fn a_derived_input_keeps_a_sound_skipped_tpl_domain() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_initial_release(
        indoc! {r#"
            {{- $name := .Values.nameOverride | default .Release.Name | trunc 63 | trimSuffix "-" -}}
            {{- contains "{{" (toJson (dict "name" $name "release" .Release.Name)) -}}
        "#},
        &db,
    );
    let mut checked = 0;
    for exit in &execution.exits {
        if !matches!(exit.flow, super::super::Flow::Normal(_)) {
            continue;
        }
        let site = exit
            .state
            .sites
            .iter()
            .last()
            .ok_or_eyre("missing containment observation")?;
        let test = exit.state.test(&site.value().value);
        assert_ne!(test.when_false, Predicate::False, "{test:?}");
        checked += 1;
    }
    assert!(checked > 0);
    Ok(())
}

#[test]
fn a_two_byte_truncation_has_an_exact_symbolic_opener_test() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for length in [2, -2] {
        let source = format!(r#"{{{{ contains "{{{{" (trunc {length} .Values.text) }}}}"#);
        let execution = execute_document(&source, None, &db);
        let site = execution
            .sites()
            .last()
            .ok_or_eyre("missing containment observation")?;
        let exit = execution
            .exits
            .iter()
            .find(|exit| matches!(exit.flow, super::super::Flow::Normal(_)))
            .ok_or_eyre("missing successful exit")?;
        let test = exit.state.test(&site.value().value);
        sim_assert_eq!(have: test.coverage, want: super::super::Coverage::Exact);
        let PredicateKind::Guard(Guard::MatchesPattern {
            path,
            pattern,
            templated: false,
        }) = test.when_true.kind()
        else {
            eyre::bail!("expected a direct input pattern: {:?}", test.when_true);
        };
        sim_assert_eq!(have: path, want: &ValuesPath::parse("text"));
        let pattern = regress::Regex::new(pattern)?;
        for text in [
            "",
            "{",
            "{{",
            "{{x",
            "x{{",
            "🙂{{",
            "{{🙂",
            "{{\n",
            "{{\r\n",
            indoc! {"

                {{"},
        ] {
            let expected = if length > 0 {
                text.starts_with("{{")
            } else {
                text.ends_with("{{")
            };
            sim_assert_eq!(have: pattern.find(text).is_some(), want: expected, "{length}: {text}");
        }
    }
    Ok(())
}

#[test]
fn host_domains_follow_heap_mutation_and_serialization_snapshots() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_initial_release(
        indoc! {r#"
            {{- $saved := toJson (dict "name" .Release.Name) -}}
            {{- $_ := set .Release "Name" "{{unsafe" -}}
            {{- contains "{{" $saved -}}|{{- contains "{{" .Release.Name -}}
        "#},
        &db,
    );
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("false|true".to_owned())]);
}

#[test]
fn a_proven_skipped_root_program_keeps_the_values_receiver() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_initial_release(
        indoc! {r#"
            {{- $name := printf "%s" (.Release.Name | trunc 63 | trimSuffix "-") -}}
            {{- if contains "{{" (toJson (dict "name" $name "release" .Release.Name)) -}}
                {{- tpl .Values.program $ -}}
            {{- end -}}
            {{- .Values.after -}}
        "#},
        &db,
    );
    let site = execution.sites().last().ok_or_eyre("missing final read")?;
    sim_assert_eq!(have: site.value().identity(), want: Some(&ValuesPath::parse("after")));
    Ok(())
}

#[test]
fn an_encoding_failure_does_not_prove_a_positive_opener_test() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        indoc! {r#"
            {{- $map := dict "{{key" "value" -}}
            {{- $_ := set $map "cycle" $map -}}
            {{- contains "{{" (toJson $map) -}}
        "#},
        None,
        &db,
    );
    let site = execution
        .sites()
        .last()
        .ok_or_eyre("missing containment observation")?;
    let test = execution
        .exits
        .first()
        .ok_or_eyre("missing exit")?
        .state
        .test(&site.value().value);
    sim_assert_eq!(have: test.when_true.clone(), want: Predicate::False);
    Ok(())
}

#[test]
fn constructed_json_retains_an_exact_template_action_condition() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_initial_release(
        indoc! {r#"
            {{- $text := toJson (dict "name" .Values.nameOverride "release" .Release.Name "items" (list .Values.other "plain")) -}}
            {{- contains "{{" $text -}}
        "#},
        &db,
    );
    let site = execution
        .sites()
        .last()
        .ok_or_eyre("missing containment observation")?;
    let test = execution
        .exits
        .first()
        .ok_or_eyre("missing exit")?
        .state
        .test(&site.value().value);
    sim_assert_eq!(have: test.coverage, want: super::super::Coverage::Exact);
    assert_ne!(test.when_true, Predicate::False);
    assert_ne!(test.when_false, Predicate::False);
    Ok(())
}

#[test]
fn nonlossy_derived_text_keeps_the_exact_input_condition() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_initial_release(
        r#"{{ contains "{{" (toJson (dict "name" (quote (printf "%s-%s" (lower .Values.nameOverride) .Release.Name)))) }}"#,
        &db,
    );
    let site = execution
        .sites()
        .last()
        .ok_or_eyre("missing containment observation")?;
    let exit = execution
        .exits
        .iter()
        .find(|exit| matches!(exit.flow, super::super::Flow::Normal(_)))
        .ok_or_eyre("missing successful exit")?;
    let test = exit.state.test(&site.value().value);
    sim_assert_eq!(have: test.coverage, want: super::super::Coverage::Exact);
    let expected = Predicate::from(Guard::MatchesPattern {
        path: ValuesPath::parse("nameOverride"),
        pattern: r"\{\{".to_owned(),
        templated: false,
    });
    sim_assert_eq!(have: test.when_true.clone().normalize_boolean(), want: expected.clone().normalize_boolean());
    sim_assert_eq!(have: test.when_false.clone().normalize_boolean(), want: expected.negated().normalize_boolean());
    Ok(())
}

#[test]
fn quoting_raw_values_keeps_their_opener_condition_and_nil_omission() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ contains "{{" (print "{" (quote .Values.text) "{") }}"#,
        None,
        &db,
    );
    let exit = execution.exits.first().ok_or_eyre("missing exit")?;
    let site = exit
        .state
        .sites
        .iter()
        .last()
        .ok_or_eyre("missing containment observation")?;
    let test = exit.state.test(&site.value().value);
    let path = ValuesPath::parse("text");
    let unsafe_text = Predicate::Or(vec![
        Predicate::from(Guard::Absent { path: path.clone() }),
        Predicate::from(Guard::MatchesValue {
            path: path.clone(),
            pattern: ValuePattern::TemplateActionFree,
        }),
    ])
    .negated();
    let expected = Predicate::Or(vec![
        unsafe_text,
        super::super::kinds::input_nil_predicate(&path),
    ]);
    sim_assert_eq!(have: test.coverage, want: super::super::Coverage::Exact);
    sim_assert_eq!(have: test.when_true.clone().normalize_boolean(), want: expected.normalize_boolean());
    Ok(())
}

#[test]
fn literal_openers_execute_tpl_and_unconditional_root_tpl_still_invalidates() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_initial_release(
        indoc! {r#"
            {{- $_ := set .Values "nameOverride" "{{ $_ := set .Values `after` `written` }}" -}}
            {{- $text := .Values.nameOverride | default "plain" | trunc 128 | trimSuffix "-" -}}
            {{- if contains "{{" (toJson (dict "name" $text "release" .Release.Name)) -}}
                {{- tpl $text $ -}}
            {{- end -}}
            {{- .Values.after -}}
        "#},
        &db,
    );
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("written".to_owned())]);
    let execution = execute_initial_release(r"{{ tpl .Values.program $ }}{{ .Values.after }}", &db);
    let site = execution.sites().last().ok_or_eyre("missing final read")?;
    sim_assert_eq!(have: site.value().identity(), want: None);
    Ok(())
}

#[test]
fn symbolic_print_does_not_freeze_an_unknown_separator() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        indoc! {r"data:
          value: '{{ print .Values.a .Values.b }}'
        "},
        None,
        &db,
    );
    let exit = execution
        .exits
        .first()
        .ok_or_eyre("missing execution exit")?;
    // Both inputs can be strings containing one apostrophe: Helm emits four adjacent quotes.
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    assert!(!exit.state.output.parts.is_empty());
    Ok(())
}

#[test]
fn release_reads_abstain_until_shared_unit_effects_are_imported() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(r#"{{ contains "{{" .Release.Name }}"#, None, &db);
    let output = execution
        .sites()
        .last()
        .ok_or_eyre("missing host observation")?;
    sim_assert_eq!(have: output.value().exact_text(), want: None);
    Ok(())
}

fn execute_initial_release(source: &str, db: &IrAnalysisDb) -> crate::value::DocumentExecution {
    use crate::value::{DocumentExecution, Field, FileOutput, Remainder, State, Value};
    let mut state = State::new(Value::nil());
    let root = state.heap.map(Remainder::Closed);
    if let Some(target) = state.map_target(&root) {
        let release = crate::value::hosts::release(&mut state, true);
        state
            .heap
            .write(&target, "Release", Field::Present(release));
        let values = Value::input(ValuesPath::default());
        state.heap.write(&target, "Values", Field::Present(values));
    }
    let occurrence = crate::value::session::FileOccurrence::new(&state);
    state.enter_file_call(&root, &occurrence);
    let parsed = db.parsed_source(source, None, "gotpl");
    let exits = crate::value::eval::Executor { db }.parsed_file_body(
        source,
        None,
        parsed.root.syntax(),
        state,
        &occurrence,
    );
    DocumentExecution::new(exits, FileOutput::Manifest)
}

#[test]
fn quoted_falsy_inputs_certify_substring_absence_without_deciding_live_payloads() -> eyre::Result<()>
{
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ contains "home" (quote .Values.volumes) }}"#,
        Some("templates/NOTES.txt"),
        &db,
    );
    let observed = execution
        .sites()
        .last()
        .ok_or_eyre("missing contains observation")?;
    let test = execution
        .exits
        .first()
        .ok_or_eyre("missing exit")?
        .state
        .test(&observed.value().value);
    sim_assert_eq!(have: test.coverage, want: super::super::Coverage::CertifiedPartial);
    sim_assert_eq!(have: test.when_true.clone(), want: Predicate::False);
    sim_assert_eq!(have: test.when_false.clone(), want: Predicate::truthy_path("volumes").negated());
    Ok(())
}
