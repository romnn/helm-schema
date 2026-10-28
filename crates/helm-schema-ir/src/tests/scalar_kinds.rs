use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{GuardValue, Predicate, ValuesPath};
use indoc::indoc;
use std::collections::BTreeSet;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::scalar_value::{ScalarContent, ScalarValueDispatch};
use crate::value::kinds::GoType;

fn execute(source: &str) -> crate::value::DocumentExecution {
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    crate::value::tests::execute_document(source, None, &db)
}

#[test]
fn runtime_types_are_stamped_at_literal_and_semver_production() -> eyre::Result<()> {
    for (source, expected) in [
        ("true", GoType::Bool),
        ("1", GoType::Int),
        ("1.0", GoType::Float64),
        ("\"one\"", GoType::String),
        ("nil", GoType::Nil),
        ("(semver \"1.2.3\").Major", GoType::Uint64),
    ] {
        let execution = execute(&format!("{{{{ {source} }}}}"));
        let site = execution
            .sites()
            .filter(|site| site.rendered())
            .last()
            .ok_or_eyre("missing typed output")?;
        sim_assert_eq!(have: site.value().go_type(), want: expected);
    }
    Ok(())
}

#[test]
fn type_only_projection_is_absent_but_legacy_empty_projection_stays_present() {
    let dispatch = ScalarValueDispatch::typed(GoType::Float64);
    sim_assert_eq!(have: dispatch.content().is_none(), want: true);
    let mut empty = ScalarValueDispatch::bind_content(Vec::new(), false);
    empty.refine_type(GoType::String);
    sim_assert_eq!(have: empty.content().map(|content| (content.arms().count(), content.complete)), want: Some((0, false)));
}

#[test]
fn type_refinement_preserves_all_non_type_result_facts() -> eyre::Result<()> {
    let execution = execute("{{ float64 .Values.input }}");
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing converted output")?;
    sim_assert_eq!(have: site.value().go_type(), want: GoType::Float64);
    sim_assert_eq!(have: site.value().inputs(), want: BTreeSet::from([ValuesPath::parse("input")]));
    sim_assert_eq!(have: site.value().literal(), want: None);
    sim_assert_eq!(have: execution.reads().iter().map(|read| read.values_path.clone()).collect::<BTreeSet<_>>(), want: BTreeSet::from([ValuesPath::parse("input")]));
    Ok(())
}

#[test]
fn conflicting_types_join_without_changing_equal_content() -> eyre::Result<()> {
    let mut literal = ScalarValueDispatch::constant(GuardValue::Int(1));
    literal.refine_type(GoType::Int);
    let mut component = ScalarValueDispatch::constant(GuardValue::Int(1));
    component.refine_type(GoType::Uint64);
    sim_assert_eq!(have: literal.content_eq(&component), want: true);
    sim_assert_eq!(have: literal == component, want: false);
    let joined = literal
        .join_equal_content([&component])
        .ok_or_eyre("content join failed")?;
    sim_assert_eq!(have: joined.content_eq(&literal), want: true);
    let mut expected = literal.clone();
    expected.refine_type(GoType::Unknown);
    sim_assert_eq!(have: joined, want: expected);
    sim_assert_eq!(have: joined.constant_value(), want: Some(GuardValue::Int(1)));
    Ok(())
}

#[test]
fn type_refinement_keeps_content_and_its_incomplete_coverage() {
    let mut dispatch = ScalarValueDispatch::bind_content(
        vec![(
            Predicate::truthy_path("enabled"),
            ScalarContent::Literal(GuardValue::Int(1)),
        )],
        false,
    );
    dispatch.refine_type(GoType::Int);
    sim_assert_eq!(have: dispatch.content().map(|content| (content.arms().count(), content.complete)), want: Some((1, false)));
    sim_assert_eq!(have: dispatch.constant_value(), want: None);
}

#[test]
fn refining_partial_content_does_not_change_an_exhausted_transform_projection() {
    let mut dispatch = ScalarValueDispatch::bind_content(
        vec![(
            Predicate::truthy_path("enabled"),
            ScalarContent::Length(Box::new(ScalarContent::Identity(
                helm_schema_core::ValuesPath::parse("input"),
            ))),
        )],
        false,
    );
    dispatch.refine_type(GoType::Int);
    sim_assert_eq!(have: dispatch.stringified().content().map(|content| (content.arms().count(), content.complete)), want: Some((0, false)));
    sim_assert_eq!(have: dispatch.printf_string("%s").is_none(), want: true);
}

fn branch_program(count: usize, literal_first: bool) -> String {
    let mut source = String::from("{{- $value := float64 .Values.input -}}");
    for index in 0..count {
        let keyword = if index == 0 { "if" } else { "else if" };
        source.push_str("{{- ");
        source.push_str(keyword);
        source.push_str(" eq .Values.branch ");
        source.push_str(&index.to_string());
        source.push_str(" -}}");
        if index == 0 && literal_first {
            source.push_str("{{- $value = 1 -}}");
        } else {
            source.push_str("{{- $value = float64 .Values.input -}}");
        }
    }
    source.push_str("{{- end -}}{{- $value -}}");
    source
}

#[test]
fn exhausting_type_only_coverage_keeps_legacy_absence() {
    let execution = execute(&branch_program(129, false));
    let sites = execution
        .sites()
        .filter(|site| site.rendered())
        .collect::<Vec<_>>();
    assert!(!sites.is_empty(), "no completed output after bounded join");
    for site in sites {
        sim_assert_eq!(have: site.value().literal(), want: None);
        assert!(site.value().inputs().contains(&ValuesPath::parse("input")));
    }
}

#[test]
fn exhausting_type_only_coverage_retains_content_guards() {
    let execution = execute(&branch_program(130, true));
    let expected = Predicate::from(helm_schema_core::Guard::Eq {
        path: ValuesPath::parse("branch"),
        value: GuardValue::Int(0),
    });
    let mut found = false;
    for site in execution.sites().filter(|site| site.rendered()) {
        for alternative in site.value().alternatives() {
            if alternative.value().literal() == Some(&GuardValue::Int(1)) {
                let condition = Predicate::all(vec![
                    site.condition().clone(),
                    alternative.condition().clone(),
                ])
                .normalize_boolean();
                sim_assert_eq!(have: condition, want: expected);
                assert!(alternative.coverage_exact());
                found = true;
            }
        }
    }
    assert!(found, "bounded joins lost the exact literal arm");
}

#[test]
fn a_type_only_helper_shadow_preserves_legacy_binding_and_scope_updates() {
    let baseline = execute(r#"{{- $value := dict "a" .Values.input -}}{{- $value -}}"#);
    let actual = execute(indoc! {r#"
        {{- $value := dict "a" .Values.input -}}
        {{- if true -}}
          {{- $value := float64 .Values.number -}}
        {{- end -}}
        {{- $value -}}
    "#});
    let output = |execution: &crate::value::DocumentExecution| {
        execution
            .sites()
            .filter(|site| site.rendered())
            .last()
            .and_then(|site| site.value().read_field("a").present())
            .map(|value| value.inputs())
    };
    sim_assert_eq!(have: output(&actual), want: output(&baseline));
    assert!(output(&actual).is_some());
}
