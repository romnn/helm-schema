use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{DefineIndex, TemplateExpr, parse_action_expressions};
use helm_schema_core::Predicate;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::fragment_expr_eval::{
    FragmentEvalContext, context_value_from_outer_expr,
    helper_result_from_expr_with_fragment_locals,
};
use crate::helper_meta::HelperOutputMeta;
use test_util::prelude::sim_assert_eq;

#[test]
fn wrapped_with_program_keeps_exact_else_requirements() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = "program: |-\n  {{- with $tenants := .Values.tenants }}{{ $tenants }}{{- else }}{{ required \"username required\" .Values.auth.username }}{{- end }}\n";
    let document = context.eval_document_fragment(source);
    assert!(
        !document.fail_conditions.is_empty()
            && document
                .fail_conditions
                .iter()
                .all(|capture| !capture.contains_approximation()),
        "{document:#?}"
    );
}

#[test]
fn wrapped_nested_tenant_program_reaches_the_with_alternative() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = r#"program: |-
  {{- with $tenants := .Values.loki.tenants }}
    {{- range $tenant := $tenants }}
      {{- required "tenant name" $tenant.name }}
    {{- end }}
  {{- else }}
    {{- required "username" .Values.gateway.basicAuth.username }}
    {{- required "password" .Values.gateway.basicAuth.password }}
  {{- end }}
"#;
    let signals = context
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();
    for path in ["gateway.basicAuth.username", "gateway.basicAuth.password"] {
        assert!(
            signals.terminal_clauses().iter().any(|clause| clause
                .iter()
                .flat_map(helm_schema_core::ConditionalGuard::value_paths)
                .any(|guard_path| guard_path == path)),
            "missing required terminal for {path}: {signals:#?}"
        );
    }
}

#[test]
fn constructed_finite_tpl_program_executes_its_required_call() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = r#"{{- $program := print "{{" " required \"name\" .Values.name " "}}" -}}
apiVersion: v1
kind: ConfigMap
data:
  value: {{ tpl $program . | quote }}
"#;
    let signals = context
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();
    assert!(
        signals.terminal_clauses().iter().any(|clause| clause
            .iter()
            .flat_map(helm_schema_core::ConditionalGuard::value_paths)
            .any(|path| path == "name")),
        "the constructed program's required call must remain executable: {signals:#?}"
    );
}

#[test]
fn finite_range_append_accumulator_reaches_the_terminal_clause() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = r#"
        {{- $keys := list "ebpf" "gvisor" -}}
        {{- $found := list -}}
        {{- range $key := $keys -}}
          {{- if hasKey $.Values.driver $key -}}
            {{- $found = append $found $key -}}
          {{- end -}}
        {{- end -}}
        {{- if gt (len $found) 0 -}}
          {{- fail "removed" -}}
        {{- end -}}
        "#;
    let document = context.eval_document_fragment(source);
    let signals = context
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();

    assert!(
        ["driver.ebpf", "driver.gvisor"].iter().all(|path| {
            signals.terminal_clauses().iter().any(|clause| {
                clause.iter().any(|guard| {
                    guard
                        .value_paths()
                        .into_iter()
                        .any(|guard_path| guard_path == *path)
                })
            })
        }),
        "finite append accumulation must preserve every presence alternative: {signals:#?}; document={document:#?}"
    );
}

#[test]
fn constructed_selector_tpl_program_drives_a_caller_fail() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = r#"{{- $dep := "telemetry.v2.stackdriver.disableOutbound" -}}
{{- $res := tpl (print "{{" (repeat (split "." $dep | len) "(") ".Values." (replace "." ")." $dep) ")}}") $ -}}
{{- if not (eq $res "") -}}
{{- fail "removed" -}}
{{- end -}}
"#;
    let signals = context
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();
    assert!(
        signals
            .terminal_clauses()
            .iter()
            .any(|clause| clause.iter().any(|guard| matches!(guard,
                helm_schema_core::ConditionalGuard::NotEq { path, value }
                    if path == "telemetry.v2.stackdriver.disableOutbound"
                        && value == &helm_schema_core::GuardValue::string("")))),
        "the constructed selector program must reach the caller comparison and fail: {signals:#?}"
    );
}

#[test]
fn tpl_wrapped_helper_dispatch_drives_a_disjunctive_caller_guard() {
    let helpers = r#"
{{- define "provider.name" -}}
{{- if eq (typeOf .Values.provider) "string" -}}
{{- .Values.provider -}}
{{- else -}}
{{- .Values.provider.name -}}
{{- end -}}
{{- end -}}
"#;
    let source = r#"
{{- $provider_name := tpl (include "provider.name" .) $ -}}
{{- if eq $provider_name "webhook" -}}
{{- fail "webhook selected" -}}
{{- end -}}
"#;
    let mut defines = DefineIndex::new();
    defines.add_file_source("<inline:0>", helpers);
    let signals = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();

    assert!(
        signals.terminal_clauses().iter().any(|clause| {
            clause.iter().any(|guard| {
                matches!(guard, helm_schema_core::ConditionalGuard::AnyOf(alternatives)
                    if alternatives.len() == 2)
            })
        }),
        "the helper's type-dispatched output must remain a disjunction at the caller: {signals:#?}"
    );
}

fn single_expr(action: &str) -> TemplateExpr {
    let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
    sim_assert_eq!(have: exprs.len(), want: 1, "expected exactly one parsed expression");
    exprs.into_iter().next().expect("expression exists")
}

fn empty_context(analysis_db: &IrAnalysisDb) -> FragmentEvalContext<'_> {
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

fn helper_context(analysis_db: &IrAnalysisDb) -> FragmentEvalContext<'_> {
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
        r"$ctx.config.name | toYaml | fromYaml",
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
fn bound_helper_break_keeps_priority_candidate_conditions() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        r#"{{- define "select.context" -}}
{{- $result := dict -}}
{{- range . -}}
  {{- if and (hasKey . "securityContexts") (hasKey .securityContexts "pod") .securityContexts.pod -}}
    {{- $result = .securityContexts.pod -}}
    {{- break -}}
  {{- end -}}
  {{- if and (hasKey . "securityContext") .securityContext -}}
    {{- $result = .securityContext -}}
    {{- break -}}
  {{- end -}}
{{- end -}}
{{- toYaml $result -}}
{{- end -}}"#,
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let expr = single_expr(r#"include "select.context" (list .Values.worker .Values)"#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        None,
        None,
        context,
        &mut seen,
    );
    let legacy = result
        .effects
        .helper_rendered
        .iter()
        .find(|row| row.path == "worker.securityContext")
        .expect("worker legacy candidate");
    assert!(
        legacy.meta.predicates.iter().any(|branch| {
            branch.iter().any(|predicate| {
                matches!(predicate, Predicate::Not(_))
                    && predicate
                        .value_paths()
                        .contains("worker.securityContexts.pod")
            })
        }),
        "the legacy candidate must require every earlier break condition to be false: {legacy:#?}"
    );
    assert!(
        result
            .effects
            .helper_rendered
            .iter()
            .flat_map(|row| &row.meta.predicates)
            .flatten()
            .all(|predicate| *predicate != Predicate::False),
        "structural hasKey predicates must resolve against the active range dot: {result:#?}"
    );
}

#[test]
fn bound_helper_continue_suppresses_the_rest_of_only_that_iteration() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        r#"{{- define "loop.values" -}}
{{- range . -}}
  {{- if .skip -}}{{- continue -}}{{- end -}}
  {{- .payload -}}
{{- end -}}
{{- end -}}"#,
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let expr = single_expr(r#"include "loop.values" (list .Values.first .Values.second)"#);
    let mut seen = HashSet::new();
    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        None,
        None,
        helper_context(&analysis_db),
        &mut seen,
    );

    for (path, skip) in [
        ("first.payload", "first.skip"),
        ("second.payload", "second.skip"),
    ] {
        let row = result
            .effects
            .helper_rendered
            .iter()
            .find(|row| row.path == path)
            .unwrap_or_else(|| panic!("missing {path} row: {result:#?}"));
        assert!(
            row.meta
                .predicates
                .iter()
                .any(|branch| { branch.contains(&Predicate::truthy_path(skip).negated()) }),
            "the post-continue output must run only when {skip} is false: {row:#?}"
        );
    }
}

#[test]
fn inner_range_break_does_not_exit_the_outer_range() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        r#"{{- define "nested.loop" -}}
{{- range . -}}
  {{- range (list "only") -}}{{- break -}}{{- end -}}
  {{- .payload -}}
{{- end -}}
{{- end -}}"#,
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let expr = single_expr(r#"include "nested.loop" (list .Values.first .Values.second)"#);
    let mut seen = HashSet::new();
    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        None,
        None,
        helper_context(&analysis_db),
        &mut seen,
    );
    let paths = result
        .effects
        .helper_rendered
        .iter()
        .map(|row| row.path.as_str())
        .collect::<BTreeSet<_>>();

    sim_assert_eq!(have: paths, want: BTreeSet::from(["first.payload", "second.payload"]));
}

#[test]
fn bound_helper_keeps_join_shape_erasure_from_range_header() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        r#"{{- define "prometheus.namespaces" -}}
{{- $namespaces := list }}
{{- if and .Values.rbac.create .Values.server.useExistingClusterRoleName }}
  {{- if .Values.server.namespaces -}}
    {{- range $ns := join "," .Values.server.namespaces | split "," }}
      {{- $namespaces = append $namespaces (tpl $ns $) }}
    {{- end -}}
  {{- end -}}
{{- end -}}
{{ mustToJson $namespaces }}
{{- end -}}"#,
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let expr = single_expr(r#"include "prometheus.namespaces" ."#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        None,
        None,
        context,
        &mut seen,
    );

    assert!(
        result
            .effects
            .shape_erased_paths
            .contains("server.namespaces"),
        "join's total conversion must survive the helper summary: {result:#?}",
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
        crate::analysis_db::OuterRootFacts::default(),
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

#[test]
fn yaml_helper_output_preserves_structured_value_for_decoding() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        r#"{{- define "pod.template" -}}
metadata:
  labels:
    app: test
spec:
  hostUsers: {{ .Values.hostUsers }}
{{- end -}}"#,
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let expr = single_expr(r#"include "pod.template" . | fromYaml"#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        None,
        None,
        context,
        &mut seen,
    );
    let value = result.value.expect("decoded helper mapping");
    sim_assert_eq!(
        have: value.apply_to_path(&["spec".to_string(), "hostUsers".to_string()]),
        want: Some(AbstractValue::OutputPath(
            "hostUsers".to_string(),
            HelperOutputMeta {
                provenance: vec![crate::ContractProvenance::new(
                    "<inline:0>".to_string(),
                    crate::SourceSpan::new(83, 106),
                    vec!["pod.template".to_string()],
                )],
                ..HelperOutputMeta::default()
            },
        )),
    );
}
