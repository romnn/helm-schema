use helm_schema_ast::DefineIndex;
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use super::{AbortReason, Datum, Flow, MapType, Value};
use crate::analysis_db::IrAnalysisDb;
use crate::function_semantics::Method;

#[test]
fn values_table_without_arguments_aborts_before_reading_a_shadow_key() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = crate::value::tests::execute_document(
        r#"{{ $_ := set .Values "Table" (dict "key" "value") }}{{ .Values.Table }}"#,
        None,
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::True);
    assert!(
        execution
            .exits
            .iter()
            .all(|exit| matches!(exit.flow, Flow::Abort(AbortReason::Arity)))
    );
}

#[test]
fn values_method_arity_is_checked_before_argument_evaluation() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution =
        crate::value::tests::execute_document(r#"{{ .Values.AsMap (fail "later") }}"#, None, &db);
    assert!(!execution.exits.is_empty());
    assert!(
        execution
            .exits
            .iter()
            .all(|exit| matches!(exit.flow, Flow::Abort(AbortReason::Arity)))
    );
}

#[test]
fn nested_map_keys_are_not_values_methods() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = crate::value::tests::execute_document(
        r#"{{ $_ := set .Values "nested" (dict "Table" "kept") }}{{ .Values.nested.Table }}"#,
        None,
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("kept".to_owned())]);
}

#[test]
fn values_asmap_keeps_nonempty_map_aliases() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = crate::value::tests::execute_document(
        r#"{{ $_ := set .Values "key" "old" }}{{ $m := .Values.AsMap }}{{ $_ := set $m "key" "new" }}{{ .Values.key }}"#,
        None,
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("new".to_owned())]);
}

#[test]
fn method_descriptors_keep_deferred_receivers_foreign() {
    let values = Value::leaf(Datum::Map {
        view: MapType::Values,
        target: None,
    });
    let ordinary = Value::leaf(Datum::Map {
        view: MapType::Interface,
        target: None,
    });
    sim_assert_eq!(have: super::hosts::method(&values, "Table"), want: Method::ValuesTable);
    sim_assert_eq!(have: super::hosts::method(&ordinary, "Table"), want: Method::Foreign("Table".to_owned()));
    sim_assert_eq!(have: super::hosts::method(&Value::nil(), "Get"), want: Method::Foreign("Get".to_owned()));
}

#[test]
fn closed_chart_root_preserves_every_engine_field_and_unknown_host_type() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = crate::value::tests::execute_document(
        indoc::indoc! {r#"
            {{- hasKey . "Values" -}}{{- hasKey . "Chart" -}}
            {{- hasKey . "Release" -}}{{- hasKey . "Capabilities" -}}
            {{- hasKey . "Files" -}}{{- hasKey . "Subcharts" -}}
            {{- hasKey . "Template" -}}{{- hasKey . "custom" -}}
        "#},
        None,
        &db,
    );
    sim_assert_eq!(have: execution.text().map(|(_, value)| value.exact_text()).collect::<Vec<_>>(), want: vec![Some("truetruetruetruetruetruetruefalse".to_string())]);
    let execution = crate::value::tests::execute_document(
        r#"{{ if kindIs "map" .Capabilities }}{{ fail "not a map" }}{{ end }}"#,
        None,
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
}
