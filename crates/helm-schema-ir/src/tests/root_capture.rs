use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{DefineIndex, TemplateExpr};
use helm_schema_core::ValuesPath;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::capture_root_context;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{bindings_from_helper_arg_value, eval_expr};
use crate::{SymbolicIrContext, SymbolicPolicy};

fn text(value: &str) -> AbstractValue {
    AbstractValue::StringSet(BTreeSet::from([value.to_string()]))
}

fn fields() -> BTreeMap<String, AbstractValue> {
    BTreeMap::from([
        (
            "Chart".to_string(),
            AbstractValue::Dict(BTreeMap::from([("Name".to_string(), text("parent"))])),
        ),
        (
            "Template".to_string(),
            AbstractValue::Dict(BTreeMap::from([(
                "BasePath".to_string(),
                text("parent/templates"),
            )])),
        ),
    ])
}

#[test]
fn captured_root_is_closed_with_explicit_values_identity() {
    let captured = capture_root_context(AbstractValue::RootContext, &fields());
    let mut entries = fields();
    entries.insert("Values".to_string(), AbstractValue::values_root());
    sim_assert_eq!(
        have: captured.clone(),
        want: AbstractValue::Overlay {
            entries: entries.clone(),
            fallback: Box::new(AbstractValue::Unknown),
        }
    );

    // A later frame's unmodeled Release cannot fill a field on the original root.
    let replacement = HashMap::from([(
        "Release".to_string(),
        AbstractValue::Dict(BTreeMap::from([("Name".to_string(), text("child"))])),
    )]);
    sim_assert_eq!(
        have: bindings_from_helper_arg_value(Some(captured), Some(&replacement)),
        want: entries.into_iter().collect::<HashMap<_, _>>()
    );
}

#[test]
fn captured_values_reads_keep_root_member_host_captures() {
    let expression = TemplateExpr::Field(vec![
        "Values".to_string(),
        "config".to_string(),
        "enabled".to_string(),
    ]);
    let direct = eval_expr(&expression, &EvalEnv::default());
    let captured = capture_root_context(AbstractValue::RootContext, &BTreeMap::new());
    let env = EvalEnv::from_helper_context(
        None,
        Some(&captured),
        crate::eval_env::BindingEvaluationMode::Evaluated,
    );
    let actual = eval_expr(&expression, &env);
    assert!(!direct.effects.observed_facts.captures.is_empty());
    sim_assert_eq!(have: actual.effects.observed_facts.captures, want: direct.effects.observed_facts.captures);
}

#[test]
fn captured_values_reads_keep_prefixed_member_host_captures() {
    let expression = TemplateExpr::Field(vec![
        "Values".to_string(),
        "config".to_string(),
        "enabled".to_string(),
    ]);
    let direct_expression = TemplateExpr::Field(vec![
        "Values".to_string(),
        "kid".to_string(),
        "config".to_string(),
        "enabled".to_string(),
    ]);
    let direct = eval_expr(&direct_expression, &EvalEnv::default());
    let captured = capture_root_context(
        AbstractValue::RootContext,
        &BTreeMap::from([(
            "Values".to_string(),
            AbstractValue::ValuesPath(ValuesPath::parse("kid")),
        )]),
    );
    let env = EvalEnv::from_helper_context(
        None,
        Some(&captured),
        crate::eval_env::BindingEvaluationMode::Evaluated,
    );
    let actual = eval_expr(&expression, &env);
    sim_assert_eq!(have: actual.effects.observed_facts.captures, want: direct.effects.observed_facts.captures);
}

#[test]
fn constructed_captured_values_maps_do_not_require_raw_parent_hosts() {
    let expression = TemplateExpr::Field(vec![
        "Values".to_string(),
        "config".to_string(),
        "enabled".to_string(),
    ]);
    let values = AbstractValue::Dict(BTreeMap::from([(
        "config".to_string(),
        AbstractValue::Dict(BTreeMap::from([("enabled".to_string(), text("yes"))])),
    )]));
    let captured = capture_root_context(
        AbstractValue::RootContext,
        &BTreeMap::from([("Values".to_string(), values)]),
    );
    let env = EvalEnv::from_helper_context(
        None,
        Some(&captured),
        crate::eval_env::BindingEvaluationMode::Evaluated,
    );
    let actual = eval_expr(&expression, &env);
    sim_assert_eq!(have: actual.value, want: Some(text("yes")));
    sim_assert_eq!(have: actual.effects.observed_facts.captures, want: BTreeSet::new());
}

#[test]
fn nested_root_capture_is_idempotent_across_replacement_frames() {
    let original = AbstractValue::Dict(BTreeMap::from([(
        "roots".to_string(),
        AbstractValue::List(vec![AbstractValue::RootContext]),
    )]));
    let captured = capture_root_context(original, &fields());
    let root = AbstractValue::Overlay {
        entries: fields()
            .into_iter()
            .chain([("Values".to_string(), AbstractValue::values_root())])
            .collect(),
        fallback: Box::new(AbstractValue::Unknown),
    };
    sim_assert_eq!(
        have: captured.clone(),
        want: AbstractValue::Dict(BTreeMap::from([(
            "roots".to_string(),
            AbstractValue::List(vec![root]),
        )]))
    );
    let replacement = BTreeMap::from([("Release".to_string(), text("child"))]);
    sim_assert_eq!(
        have: capture_root_context(captured.clone(), &replacement),
        want: captured
    );
}

#[test]
fn captured_mutations_preserve_values_prefix_and_existing_overrides() {
    let mutated = AbstractValue::RootContext.with_overlay_entries(BTreeMap::from([
        (
            "Values".to_string(),
            AbstractValue::ValuesPath(ValuesPath::parse("kid")),
        ),
        (
            "Template".to_string(),
            AbstractValue::Dict(BTreeMap::from([(
                "BasePath".to_string(),
                text("kid/templates"),
            )])),
        ),
    ]));
    let captured = capture_root_context(mutated, &fields());
    let mut expected_fields = fields();
    expected_fields.insert(
        "Values".to_string(),
        AbstractValue::ValuesPath(ValuesPath::parse("kid")),
    );
    expected_fields.insert(
        "Template".to_string(),
        AbstractValue::Dict(BTreeMap::from([(
            "BasePath".to_string(),
            text("kid/templates"),
        )])),
    );
    sim_assert_eq!(
        have: bindings_from_helper_arg_value(Some(captured.clone()), Some(&HashMap::new())),
        want: expected_fields.into_iter().collect::<HashMap<_, _>>()
    );
    sim_assert_eq!(
        have: captured.apply_to_path(&["Values".to_string(), "token".to_string()]),
        want: Some(AbstractValue::ValuesPath(ValuesPath::parse("kid.token")))
    );
}

#[test]
fn explicit_captured_dot_does_not_fall_back_to_the_enclosing_root() {
    let captured = capture_root_context(AbstractValue::RootContext, &fields());
    let replacement = HashMap::from([(
        "Release".to_string(),
        AbstractValue::Dict(BTreeMap::from([("Name".to_string(), text("child"))])),
    )]);
    let env = EvalEnv::from_helper_context(
        Some(&replacement),
        Some(&captured),
        crate::eval_env::BindingEvaluationMode::Evaluated,
    );
    sim_assert_eq!(
        have: eval_expr(
            &TemplateExpr::Field(vec!["Release".to_string(), "Name".to_string()]),
            &env,
        ).value,
        want: None
    );
    let env = EvalEnv::from_helper_context(
        Some(&replacement),
        Some(&AbstractValue::RootContext),
        crate::eval_env::BindingEvaluationMode::Evaluated,
    );
    sim_assert_eq!(
        have: eval_expr(
            &TemplateExpr::Field(vec!["Release".to_string(), "Name".to_string()]),
            &env,
        ).value,
        want: Some(text("child"))
    );
}

fn dispatch_context(condition: &str) -> SymbolicIrContext {
    let mut index = DefineIndex::new();
    index.add_file_source(
        "parent/templates/_bridge.tpl",
        r#"{{ define "bridge" }}{{ include "choose" .original }}{{ end }}"#,
    );
    index.add_file_source(
        "parent/templates/_choose.tpl",
        &indoc::formatdoc! {r#"
            {{{{- define "choose" -}}}}
            {{{{- if {condition} -}}}}
            {{{{- required "need parent" .Values.parentToken -}}}}
            {{{{- else -}}}}
            {{{{- required "need child" .Values.childToken -}}}}
            {{{{- end -}}}}
            {{{{- end -}}}}
        "#},
    );
    SymbolicIrContext::with_policy(
        &index,
        SymbolicPolicy {
            static_root_strings: BTreeMap::from([
                (
                    vec!["Chart".to_string(), "Name".to_string()],
                    "parent".to_string(),
                ),
                (
                    vec!["Template".to_string(), "BasePath".to_string()],
                    "parent/templates".to_string(),
                ),
            ]),
            ..SymbolicPolicy::default()
        },
    )
}

#[test]
fn carried_unknown_release_keeps_the_direct_roots_complete_signals() {
    let context = dispatch_context(r#"eq .Release.Name "parent""#);
    let direct = context
        .generate_contract_ir(r#"{{ include "choose" . }}"#)
        .finalize()
        .into_schema_signals();
    let carried = context
        .generate_contract_ir(indoc! {r#"
        {{ include "bridge" (dict "original" $ "Release" (dict "Name" "parent")) }}
    "#})
        .finalize()
        .into_schema_signals();
    sim_assert_eq!(have: carried, want: direct);
}

#[test]
fn carried_known_metadata_keeps_the_direct_roots_complete_signals() {
    let context = dispatch_context(
        r#"and (eq .Chart.Name "parent") (eq .Template.BasePath "parent/templates")"#,
    );
    let direct = context
        .generate_contract_ir(r#"{{ include "choose" . }}"#)
        .finalize()
        .into_schema_signals();
    let carried = context.generate_contract_ir(indoc! {r#"
        {{ include "bridge" (dict "original" $ "Chart" (dict "Name" "child") "Template" (dict "BasePath" "child/templates")) }}
    "#}).finalize().into_schema_signals();
    sim_assert_eq!(have: carried, want: direct);
}

#[test]
fn caller_root_mutations_reach_capture_and_direct_passthrough() {
    let context = dispatch_context(r#"eq .Release.Name "parent""#);
    let direct = context
        .generate_contract_ir(indoc! {r#"
        {{ $_ := set . "Release" (dict "Name" "parent") }}
        {{ include "choose" . }}
    "#})
        .finalize()
        .into_schema_signals();
    let carried = context
        .generate_contract_ir(indoc! {r#"
        {{ $args := dict "original" $ }}
        {{ $_ := set . "Release" (dict "Name" "parent") }}
        {{ include "bridge" $args }}
    "#})
        .finalize()
        .into_schema_signals();
    sim_assert_eq!(have: carried, want: direct);
}
