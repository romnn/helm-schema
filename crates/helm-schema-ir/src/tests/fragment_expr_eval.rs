use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{DefineIndex, TemplateExpr, parse_action_expressions};

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::fragment_expr_eval::{
    FragmentEvalContext, context_value_from_outer_expr,
    helper_result_from_expr_with_fragment_locals,
};
use crate::helper_meta::HelperOutputMeta;
use test_util::prelude::sim_assert_eq;

fn single_expr(action: &str) -> TemplateExpr {
    let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
    sim_assert_eq!(have: exprs.len(), want: 1, "expected exactly one parsed expression");
    exprs.into_iter().next().expect("expression exists")
}

fn empty_context<'a>(analysis_db: &'a IrAnalysisDb) -> FragmentEvalContext<'a> {
    FragmentEvalContext::new(analysis_db)
}

fn helper_value_from_fragment_locals(
    action: &str,
    fragment_locals: &HashMap<String, AbstractValue>,
) -> Option<AbstractValue> {
    let expr = single_expr(action);
    let defines = DefineIndex::new();
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = empty_context(&analysis_db);
    let mut seen = HashSet::new();
    helper_result_from_expr_with_fragment_locals(
        &expr,
        fragment_locals,
        None,
        None,
        context,
        &mut seen,
    )
    .value
}

fn context_local() -> HashMap<String, AbstractValue> {
    HashMap::from([(
        "ctx".to_string(),
        AbstractValue::Dict(BTreeMap::from([(
            "config".to_string(),
            AbstractValue::ValuesPath("serviceAccount".to_string()),
        )])),
    )])
}

#[test]
fn printf_resolves_literal_fragment_local() {
    let strings = ["path".to_string()].into_iter().collect();
    let locals = HashMap::from([("opPathKey".to_string(), AbstractValue::StringSet(strings))]);

    sim_assert_eq!(
        have: helper_value_from_fragment_locals(r#"printf "%sKey" $opPathKey"#, &locals),
        want: Some(AbstractValue::StringSet(
            ["pathKey".to_string()].into_iter().collect()
        ))
    );
}

fn helper_context<'a>(analysis_db: &'a IrAnalysisDb) -> FragmentEvalContext<'a> {
    empty_context(analysis_db)
}

#[test]
fn outer_expr_bare_dot_uses_root_bindings_as_current_context() {
    let expr = single_expr(".");
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);

    sim_assert_eq!(
        have: context_value_from_outer_expr(&expr, None, Some(&root_bindings), None),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "Values".to_string(),
            AbstractValue::values_root(),
        )])))
    );
}

#[test]
fn outer_expr_root_variable_uses_root_bindings_as_current_context() {
    let expr = single_expr("$");
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);

    sim_assert_eq!(
        have: context_value_from_outer_expr(&expr, None, Some(&root_bindings), None),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "Values".to_string(),
            AbstractValue::values_root(),
        )])))
    );
}

#[test]
fn outer_expr_fragment_local_selector_uses_shared_expression_eval() {
    let expr = single_expr(r#"dict "name" $ctx.config.name"#);
    let fragment_locals = context_local();

    sim_assert_eq!(
        have: context_value_from_outer_expr(&expr, Some(&fragment_locals), None, None),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "name".to_string(),
            AbstractValue::ValuesPath("serviceAccount.name".to_string()),
        )])))
    );
}

#[test]
fn helper_value_fragment_local_selector_uses_shared_expression_eval() {
    let binding = helper_value_from_fragment_locals(
        r#"$ctx.config.name | toYaml | fromYaml"#,
        &context_local(),
    );

    sim_assert_eq!(
        have: binding,
        want: Some(AbstractValue::ValuesPath("serviceAccount.name".to_string()))
    );
}

#[test]
fn helper_value_fragment_local_dict_uses_shared_expression_eval() {
    let binding =
        helper_value_from_fragment_locals(r#"dict "name" $ctx.config.name"#, &context_local());

    sim_assert_eq!(
        have: binding,
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "name".to_string(),
            AbstractValue::ValuesPath("serviceAccount.name".to_string()),
        )])))
    );
}

#[test]
fn helper_value_fragment_local_index_uses_shared_expression_eval() {
    let binding =
        helper_value_from_fragment_locals(r#"index $ctx.config "name""#, &context_local());

    sim_assert_eq!(
        have: binding,
        want: Some(AbstractValue::ValuesPath("serviceAccount.name".to_string()))
    );
}

#[test]
fn bound_helper_call_uses_single_value_resolver_for_helper_projection() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        r#"{{- define "common.name" -}}{{ .Values.nameOverride }}{{- end -}}"#,
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let expr = single_expr(r#"include "common.name" ."#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        None,
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::OutputPath(
            "nameOverride".to_string(),
            HelperOutputMeta {
                predicates: BTreeSet::new(),
                defaulted: false,
                provenance: vec![crate::ContractProvenance::new(
                    "<inline:0>".to_string(),
                    crate::SourceSpan::new(28, 54),
                    vec!["common.name".to_string()],
                )],
                ..HelperOutputMeta::default()
            },
        ))
    );
    let output = result
        .effects
        .helper_rendered
        .iter()
        .find(|row| row.path == "nameOverride")
        .expect("nameOverride rendered row should be present");
    let meta = &output.meta;
    sim_assert_eq!(
        have: crate::tests::raw_guard_sets(meta, "nameOverride"),
        want: vec![Vec::new()]
    );
    assert!(!meta.defaulted);
    assert!(
        meta.provenance.iter().any(|provenance| {
            provenance.template_path == "<inline:0>"
                && provenance.helper_chain == vec!["common.name".to_string()]
                && provenance.span.start < provenance.span.end
        }),
        "expected helper projection to retain helper-body provenance, got {meta:?}",
    );
}

#[test]
fn bound_helper_call_uses_single_value_resolver_for_fragment_projection() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        r#"{{- define "common.name" -}}{{ .Values.nameOverride }}{{- end -}}"#,
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let expr = single_expr(r#"include "common.name" ."#);
    let mut seen = HashSet::new();

    sim_assert_eq!(
        have: context.fragment_value_from_expr(&expr, &HashMap::new(), None, &mut seen),
        want: Some(AbstractValue::OutputPath(
            "nameOverride".to_string(),
            HelperOutputMeta {
                predicates: BTreeSet::new(),
                defaulted: false,
                provenance: vec![crate::ContractProvenance::new(
                    "<inline:0>".to_string(),
                    crate::SourceSpan::new(28, 54),
                    vec!["common.name".to_string()],
                )],
                ..HelperOutputMeta::default()
            },
        )),
    );
}

#[test]
fn json_serialized_helper_preserves_structured_root_value_for_decoding() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        r#"{{- define "json.roundtrip" -}}
{{- $params := fromJson (toJson .) -}}
{{- $doc := pick $params "doc" -}}
{{- toJson $doc -}}
{{- end -}}"#,
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let expr = single_expr(r#"include "json.roundtrip" (dict "doc" $values) | fromJson"#);
    let mut seen = HashSet::new();
    let locals = HashMap::from([("values".to_string(), AbstractValue::values_root())]);
    sim_assert_eq!(
        have: context_value_from_outer_expr(
            &single_expr(r#"dict "doc" $values"#),
            Some(&locals),
            None,
            None,
        ),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "doc".to_string(),
            AbstractValue::values_root(),
        )]))),
    );
    let include = single_expr(r#"include "json.roundtrip" (dict "doc" $values)"#);
    let TemplateExpr::Call { args, .. } = &include else {
        panic!("include expression");
    };
    let mut summary_seen = HashSet::new();
    let call = analysis_db.summarize_bound_helper_call(
        "json.roundtrip",
        args.get(1),
        None,
        None,
        &locals,
        context,
        &mut summary_seen,
    );
    assert!(
        call.summary.value.is_some(),
        "root JSON summary should retain a value: {:#?}",
        call.summary.root
    );

    let result = helper_result_from_expr_with_fragment_locals(
        &expr, &locals, None, None, context, &mut seen,
    );
    let value = result.value.as_ref().expect("helper output value");
    let doc = value
        .apply_to_path(&["doc".to_string()])
        .unwrap_or_else(|| {
            panic!("decoded helper output should retain its doc member: {value:#?}")
        });

    sim_assert_eq!(have: doc.unique_path(), want: Some(String::new()));
    sim_assert_eq!(have: doc.unique_json_decoded_path(), want: Some(String::new()));
}
