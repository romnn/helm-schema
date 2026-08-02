use indoc::indoc;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{DefineIndex, TemplateExpr, parse_action_expressions};
use helm_schema_core::{
    ApproximationRole, ConditionalGuard, Guard, GuardDnf, GuardValue, Predicate,
};

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::eval_effect::{CaptureKind, EvalResult};
use crate::eval_env::EvalEnv;
use crate::fragment_expr_eval::{
    FragmentEvalContext, context_value_from_outer_expr, document_result_from_expr,
};
use crate::helper_meta::HelperOutputMeta;
use crate::scalar_value::{ScalarRenderPart, ScalarValue, ScalarValueDispatch, TruthCondition};
use test_util::prelude::sim_assert_eq;

fn helper_result_from_expr_with_fragment_locals(
    expr: &TemplateExpr,
    fragment_locals: &HashMap<String, AbstractValue>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> EvalResult {
    let mut env = EvalEnv::from_helper_context(outer, current_dot);
    env.locals = fragment_locals.clone();
    let mut result = document_result_from_expr(expr, &env, outer, current_dot, context, seen);
    result.value = result.value.map(|value| value.to_context_value());
    result
}

#[test]
fn wrapped_with_program_keeps_exact_else_requirements() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = indoc! {r#"
        program: |-
          {{- with $tenants := .Values.tenants }}{{ $tenants }}{{- else }}{{ required "username required" .Values.auth.username }}{{- end }}
    "#};
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
fn yaml_serialization_requires_presence_only_with_coexecuting_mapping_members() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = indoc! {r#"
        {{- if .Values.config.create }}
        data:
          {{- if .Values.config.clusterWide }}
          {{- toYaml .Values.config.data | nindent 2 }}
          {{- else }}
          {{- $namespace := dict "WATCH_NAMESPACE" .Release.Namespace }}
          {{- $data := merge .Values.config.data $namespace }}
          {{- toYaml $data | nindent 2 }}
          {{- end }}
        {{- end }}
    "#};
    let signals = context
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();
    let clauses = signals
        .terminal_clauses()
        .iter()
        .filter(|clause| {
            clause
                .iter()
                .any(|guard| guard.value_paths().iter().any(|path| path == "config.data"))
        })
        .cloned()
        .collect::<Vec<_>>();

    sim_assert_eq!(
        have: clauses,
        want: vec![vec![
            ConditionalGuard::Truthy {
                path: "config.create".to_string(),
            },
            ConditionalGuard::Absent {
                path: "config.data".to_string(),
            },
            ConditionalGuard::Not(ConditionalGuard::Truthy {
                path: "config.clusterWide".to_string(),
            }
            .into()),
        ]]
    );
}

#[test]
fn helper_yaml_serialization_keeps_its_fixed_sequence_sibling() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "pod" -}}
            volumeMounts:
              - name: config
                mountPath: /config
            {{- if eq .Values.kind "DaemonSet" }}
              {{- toYaml .Values.daemonSetVolumeMounts | nindent 2 }}
            {{- end }}
            {{- end -}}
        "#},
    );
    let context = crate::SymbolicIrContext::new(&defines);
    let signals = context
        .generate_contract_ir(r#"{{ include "pod" . }}"#)
        .finalize()
        .into_schema_signals();
    let clauses = signals
        .terminal_clauses()
        .iter()
        .filter(|clause| {
            clause.iter().any(|guard| {
                guard
                    .value_paths()
                    .iter()
                    .any(|path| path == "daemonSetVolumeMounts")
            })
        })
        .cloned()
        .collect::<Vec<_>>();

    sim_assert_eq!(
        have: clauses,
        want: vec![vec![
            ConditionalGuard::Eq {
                path: "kind".to_string(),
                value: GuardValue::string("DaemonSet"),
            },
            ConditionalGuard::Absent {
                path: "daemonSetVolumeMounts".to_string(),
            },
        ]]
    );
}

#[test]
fn direct_provider_scalar_keeps_positive_subset_of_int_cast_guard() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = indoc! {r"
        {{- if and (not .Values.sentinel.enabled) (gt (int64 .Values.master.count) 0) }}
        apiVersion: v1
        kind: Service
        metadata:
          name: redis
        spec:
          ports:
            - name: redis
              port: {{ .Values.master.service.ports.redis }}
        {{- end }}
    "};
    let contract = context.generate_contract_ir(source).finalize();
    let port = contract
        .uses()
        .iter()
        .find(|use_| use_.source_expr == "master.service.ports.redis");

    let int_gt = Predicate::from(Guard::IntGt {
        path: "master.count".to_string(),
        bound: 0,
    });
    sim_assert_eq!(
        have: port.map(|use_| use_.condition.clone()),
        want: Some(GuardDnf::from_conjunction([
            Predicate::Approximate {
                marker: "0:0:0:1".to_string(),
                paths: BTreeSet::from(["master.count".to_string()]),
                role: ApproximationRole::Control,
                sound_subset: Some(Box::new(int_gt)),
            },
            Predicate::from(Guard::Not {
                path: "sentinel.enabled".to_string(),
            }),
        ]))
    );

    let evidence = contract
        .schema_signals()
        .evidence_for("master.service.ports.redis")
        .cloned();
    sim_assert_eq!(
        have: evidence.and_then(|evidence| {
            evidence
                .conditional_overlays
                .into_iter()
                .find(|overlay| {
                    overlay.guards
                        == [
                            ConditionalGuard::IntGt {
                                path: "master.count".to_string(),
                                bound: 0,
                            },
                            ConditionalGuard::Not(Box::new(ConditionalGuard::Truthy {
                                path: "sentinel.enabled".to_string(),
                            })),
                        ]
                })
                .map(|overlay| overlay.evidence.provider_schema_uses)
        }),
        want: Some(vec![crate::ProviderSchemaUse {
            value_path: "master.service.ports.redis".to_string(),
            path: crate::YamlPath(vec![
                "spec".to_string(),
                "ports[*]".to_string(),
                "port".to_string(),
            ]),
            kind: crate::ValueKind::Scalar,
            stringified: false,
            resource: crate::ResourceRef::concrete("v1".to_string(), "Service".to_string()),
            is_self_range_collection: false,
            source_null_tolerant: false,
            template_supplied_member_keys: BTreeSet::new(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: BTreeMap::new(),
            outer_guards: Vec::new(),
        }])
    );
}

#[test]
fn yaml_serialization_scopes_presence_to_conditional_mapping_members() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = indoc! {r#"
        {{- if .Values.enabled }}
        metadata:
          annotations:
        {{ toYaml .Values.annotations | indent 4 }}
        {{- if .Values.internalTls }}
            backend-protocol: "HTTPS"
        {{- end }}
        {{- if eq .Values.controller "ncp" }}
            use-regex: "true"
        {{- end }}
        {{- end }}
    "#};
    let signals = context
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();
    let clauses = signals
        .terminal_clauses()
        .iter()
        .filter(|clause| {
            clause
                .iter()
                .any(|guard| guard.value_paths().iter().any(|path| path == "annotations"))
        })
        .cloned()
        .collect::<Vec<_>>();

    sim_assert_eq!(
        have: clauses,
        want: vec![
            vec![
                ConditionalGuard::Truthy {
                    path: "enabled".to_string(),
                },
                ConditionalGuard::Truthy {
                    path: "internalTls".to_string(),
                },
                ConditionalGuard::Absent {
                    path: "annotations".to_string(),
                },
            ],
            vec![
                ConditionalGuard::Truthy {
                    path: "enabled".to_string(),
                },
                ConditionalGuard::Eq {
                    path: "controller".to_string(),
                    value: helm_schema_core::GuardValue::string("ncp"),
                },
                ConditionalGuard::Absent {
                    path: "annotations".to_string(),
                },
            ],
        ]
    );
}

#[test]
fn defaulted_helper_merge_does_not_require_the_raw_source() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "json-pass" -}}
            {{- toJson . -}}
            {{- end -}}
            {{- define "load" -}}
            {{- $doc := dict "apiVersion" "v1" "kind" "ConfigMap" -}}
            {{- $doc = mergeOverwrite $doc (deepCopy (.merge | default dict)) -}}
            {{- get (include "json-pass" (dict "doc" $doc) | fromJson) "doc" | toYaml -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let fragment_context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let call_argument = single_expr(".Values.configMap");
    let summary_env = EvalEnv::from_helper_context(Some(&root_bindings), None);
    let mut summary_seen = HashSet::new();
    let call = analysis_db.summarize_bound_helper_call(
        "load",
        Some(&call_argument),
        Some(&root_bindings),
        None,
        &summary_env,
        fragment_context,
        &mut summary_seen,
    );
    let rendered = call
        .summary
        .rendered
        .iter()
        .filter(|row| row.path == "configMap.merge")
        .map(|row| row.meta.defaulted)
        .collect::<Vec<_>>();
    sim_assert_eq!(have: rendered, want: vec![true]);

    let context = crate::SymbolicIrContext::new(&defines);
    let source = indoc! {r#"
        {{- with .Values.configMap }}
        {{- include "load" . }}
        {{- end }}
    "#};
    let signals = context
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();
    let clauses = signals
        .terminal_clauses()
        .iter()
        .filter(|clause| {
            clause.iter().any(|guard| {
                guard
                    .value_paths()
                    .iter()
                    .any(|path| path == "configMap.merge")
            })
        })
        .cloned()
        .collect::<Vec<_>>();

    sim_assert_eq!(have: clauses, want: Vec::<Vec<ConditionalGuard>>::new());
}

#[test]
fn defaulted_helper_output_keeps_its_stringified_identity() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "workload.fullname" -}}
            {{- if .Values.fullnameOverride -}}
            {{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
            {{- else -}}
            {{- $name := default .Chart.Name .Values.nameOverride -}}
            {{- $releaseName := regexReplaceAll "(-?[^a-z\\d\\-])+-?" (lower .Release.Name) "-" -}}
            {{- if contains $name $releaseName -}}
            {{- $releaseName | trunc 63 | trimSuffix "-" -}}
            {{- else -}}
            {{- printf "%s-%s" $releaseName $name | trunc 63 | trimSuffix "-" -}}
            {{- end -}}
            {{- end -}}
            {{- end -}}
            {{- define "workload.serviceAccountName" -}}
            {{- if .Values.master.serviceAccount.create -}}
              {{ default (printf "%s-master" (include "workload.fullname" .)) .Values.master.serviceAccount.name }}
            {{- else -}}
              {{ default "default" .Values.master.serviceAccount.name }}
            {{- end -}}
            {{- end -}}
        "#},
    );
    let source = indoc! {r#"
        {{- if .Values.master.serviceAccount.create }}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: {{ template "workload.serviceAccountName" . }}
        {{- end }}
    "#};
    let contract = crate::SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize();
    let name_rows = contract
        .uses()
        .iter()
        .filter(|contract_use| {
            contract_use.source_expr == "master.serviceAccount.name"
                && contract_use.path == crate::YamlPath(vec!["metadata".into(), "name".into()])
        })
        .map(|contract_use| contract_use.stringified)
        .collect::<Vec<_>>();

    sim_assert_eq!(have: name_rows, want: vec![true]);
}

#[test]
fn wrapped_nested_tenant_program_reaches_the_with_alternative() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = indoc! {r#"
        program: |-
          {{- with $tenants := .Values.loki.tenants }}
            {{- range $tenant := $tenants }}
              {{- required "tenant name" $tenant.name }}
            {{- end }}
          {{- else }}
            {{- required "username" .Values.gateway.basicAuth.username }}
            {{- required "password" .Values.gateway.basicAuth.password }}
          {{- end }}
    "#};
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
    let source = indoc! {r#"
        {{- $program := print "{{" " required \"name\" .Values.name " "}}" -}}
        apiVersion: v1
        kind: ConfigMap
        data:
          value: {{ tpl $program . | quote }}
    "#};
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
    let source = indoc! {r#"
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
    "#};
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
    let source = indoc! {r#"
        {{- $dep := "telemetry.v2.stackdriver.disableOutbound" -}}
        {{- $res := tpl (print "{{" (repeat (split "." $dep | len) "(") ".Values." (replace "." ")." $dep) ")}}") $ -}}
        {{- if not (eq $res "") -}}
        {{- fail "removed" -}}
        {{- end -}}
    "#};
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
    let helpers = indoc! {r#"
        {{- define "provider.name" -}}
        {{- if eq (typeOf .Values.provider) "string" -}}
        {{- .Values.provider -}}
        {{- else -}}
        {{- .Values.provider.name -}}
        {{- end -}}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        {{- $provider_name := tpl (include "provider.name" .) $ -}}
        {{- if eq $provider_name "webhook" -}}
        {{- fail "webhook selected" -}}
        {{- end -}}
    "#};
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
        have: context_value_from_outer_expr(&expr, None, None, Some(&root_bindings), None),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "Values".to_string(),
            AbstractValue::values_root(),
        )])))
    );
}

#[test]
fn literal_helper_dispatch_uses_the_values_root_as_its_actual_dot() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "use-fips-images" -}}
            {{- if .useFIPSAgent -}}
            true
            {{- else -}}
            false
            {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let include = single_expr(r#"include "use-fips-images" .Values"#);
    let TemplateExpr::Call { args, .. } = &include else {
        panic!("include expression");
    };
    let mut summary_seen = HashSet::new();
    let summary_env = EvalEnv::from_helper_context(Some(&root_bindings), None);
    let call = analysis_db.summarize_bound_helper_call(
        "use-fips-images",
        args.get(1),
        Some(&root_bindings),
        None,
        &summary_env,
        context,
        &mut summary_seen,
    );
    assert!(
        call.summary.scalar_dispatch.is_some(),
        "the bound summary must retain a complete literal dispatch: {:#?}",
        call.summary
    );
    let expr = single_expr(r#"eq (include "use-fips-images" .Values) "true""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: Some(Predicate::truthy_path("useFIPSAgent")),
        "the callee's dot-relative selector must resolve through `.Values`: {result:#?}"
    );
}

#[test]
fn nested_helper_scalar_output_reuses_the_inner_dispatch() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "feature.enabled" -}}
            {{- if .Values.feature.enabled -}}
            true
            {{- else -}}
            false
            {{- end -}}
            {{- end -}}
            {{- define "feature.enabled.forwarded" -}}
            {{- include "feature.enabled" . -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"eq (include "feature.enabled.forwarded" .) "true""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: Some(Predicate::truthy_path("feature.enabled")),
        "the outer helper must preserve the inner helper's guarded scalar output: {result:#?}"
    );
}

#[test]
fn negated_include_uses_the_rendered_scalar_dispatch_truthiness() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "container-runtime-support-enabled" -}}
              {{- if and .Values.runtime.enabled (not .Values.provider.gdc) -}}
                true
              {{- else -}}
                false
              {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"not (include "container-runtime-support-enabled" .)"#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: Some(Predicate::False),
        "both helper arms render nonempty text, so the include is always truthy: {result:#?}"
    );
}

#[test]
fn helper_fail_header_uses_nested_include_rendered_truthiness() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "container-runtime-support-enabled" -}}
              {{- if and .Values.runtime.enabled (not .Values.provider.gdc) -}}
                true
              {{- else -}}
                false
              {{- end -}}
            {{- end -}}
            {{- define "validate-runtime" -}}
              {{- if and (not (include "container-runtime-support-enabled" .)) .Values.images.enabled -}}
                {{- fail "runtime support is required" -}}
              {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let fragment_context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let header = single_expr(
        r#"and (not (include "container-runtime-support-enabled" .)) .Values.images.enabled"#,
    );
    let mut seen = HashSet::from(["validate-runtime".to_string()]);
    let header_result = helper_result_from_expr_with_fragment_locals(
        &header,
        &HashMap::new(),
        Some(&root_bindings),
        Some(&AbstractValue::RootContext),
        fragment_context,
        &mut seen,
    );
    sim_assert_eq!(
        have: header_result.truth.predicate().cloned(),
        want: Some(Predicate::False),
        "the complete nested helper dispatch must decide the enclosing header: {header_result:#?}"
    );
    let call_argument = TemplateExpr::Field(Vec::new());
    let mut summary_seen = HashSet::new();
    let summary_env = EvalEnv::from_helper_context(Some(&root_bindings), None);
    let call = analysis_db.summarize_bound_helper_call(
        "validate-runtime",
        Some(&call_argument),
        Some(&root_bindings),
        None,
        &summary_env,
        fragment_context,
        &mut summary_seen,
    );
    assert!(
        !call
            .summary
            .fail_conditions
            .iter()
            .any(|capture| matches!(capture.kind, CaptureKind::Fail)),
        "the helper summary must prune the unreachable fail: {:#?}",
        call.summary
    );
    let context = crate::SymbolicIrContext::new(&defines);

    let signals = context
        .generate_contract_ir(r#"{{ include "validate-runtime" . }}"#)
        .finalize()
        .into_schema_signals();

    let impossible_fail = [
        ConditionalGuard::Truthy {
            path: "provider.gdc".to_string(),
        },
        ConditionalGuard::Truthy {
            path: "runtime.enabled".to_string(),
        },
    ];
    assert!(
        !signals
            .terminal_clauses()
            .iter()
            .any(|clause| clause.as_slice() == impossible_fail),
        "the nested include always renders nonempty text, so its negation cannot reach fail: {signals:#?}"
    );
}

#[test]
fn print_literal_helper_arms_form_an_exact_scalar_dispatch() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "feature.mode" -}}
            {{- if .Values.feature.enabled -}}
            {{- print "enabled" -}}
            {{- else -}}
            {{- print "disabled" -}}
            {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"eq (include "feature.mode" .Values) "enabled""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: Some(Predicate::truthy_path("feature.enabled")),
        "print literals must remain visible to the helper's scalar summary: {result:#?}"
    );
}

#[test]
fn semver_selected_print_helper_keeps_policy_default_dispatch() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "capabilities.ingress.apiVersion" -}}
            {{- if semverCompare "<1.14-0" (.Values.kubeVersion | default .Capabilities.KubeVersion.Version) -}}
            {{- print "extensions/v1beta1" -}}
            {{- else if semverCompare "<1.19-0" (.Values.kubeVersion | default .Capabilities.KubeVersion.Version) -}}
            {{- print "networking.k8s.io/v1beta1" -}}
            {{- else -}}
            {{- print "networking.k8s.io/v1" -}}
            {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::with_policy(
        &defines,
        crate::SymbolicPolicy {
            kubernetes_version: Some("1.29.0".to_string()),
            ..crate::SymbolicPolicy::default()
        },
    );
    let context = helper_context(&analysis_db);
    let mut root_bindings = analysis_db.static_root_fields().clone();
    root_bindings.insert(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    );
    let expr =
        single_expr(r#"eq (include "capabilities.ingress.apiVersion" .) "networking.k8s.io/v1""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );
    let below = |constraint| {
        helm_schema_ast::semver_constraint_match_pattern(constraint).map(|pattern| {
            Predicate::all(vec![
                Predicate::truthy_path("kubeVersion"),
                Predicate::from(Guard::MatchesPattern {
                    path: "kubeVersion".to_string(),
                    pattern,
                    templated: false,
                }),
            ])
        })
    };
    let want = below("<1.14-0")
        .zip(below("<1.19-0"))
        .map(|(below_114, below_119)| {
            Predicate::And(vec![below_114.negated(), below_119.negated()])
        });

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: want,
        "the policy fallback and truthy override must select the exact helper arm: {result:#?}"
    );
}

#[test]
fn root_context_forwarding_keeps_static_capability_scalars() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "capabilities.select" -}}
            {{- if semverCompare "^1.6-0" .Capabilities.KubeVersion.GitVersion -}}
            {{- print "modern" -}}
            {{- else -}}
            {{- print "legacy" -}}
            {{- end -}}
            {{- end -}}
            {{- define "capabilities.forward" -}}
            {{- include "capabilities.select" .context -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::with_policy(
        &defines,
        crate::SymbolicPolicy {
            kubernetes_version: Some("1.35.0".to_string()),
            ..crate::SymbolicPolicy::default()
        },
    );
    let context = helper_context(&analysis_db);
    let mut root_bindings = analysis_db.static_root_fields().clone();
    root_bindings.insert(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    );
    let expr = single_expr(r#"eq (include "capabilities.forward" (dict "context" .)) "modern""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: Some(Predicate::True),
        "forwarding the root through a helper dictionary must retain immutable policy fields: {result:#?}"
    );
}

#[test]
fn helper_local_reassignments_join_into_one_scalar_dispatch() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "feature.local" -}}
            {{- $enabled := "false" -}}
            {{- if .Values.feature.enabled -}}
            {{- $enabled = "true" -}}
            {{- end -}}
            {{- $enabled -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"eq (include "feature.local" .) "true""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: Some(Predicate::truthy_path("feature.enabled")),
        "the local's branch-selected value must survive the helper boundary: {result:#?}"
    );
}

#[test]
fn helper_range_fallback_retains_the_root_provider_candidate() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "select.container-context" -}}
            {{- $ := last . -}}
            {{- $result := dict -}}
            {{- range . -}}
              {{- if and (hasKey . "securityContexts") (hasKey .securityContexts "container") .securityContexts.container -}}
                {{- $result = .securityContexts.container -}}
                {{- break -}}
              {{- end -}}
            {{- end -}}
            {{- if $result -}}
              {{- toYaml $result -}}
            {{- else if and (hasKey $ "securityContexts") (hasKey $.securityContexts "containers") $.securityContexts.containers -}}
              {{- toYaml $.securityContexts.containers -}}
            {{- else -}}
            allowPrivilegeEscalation: false
            {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"include "select.container-context" (list .Values.worker .Values)"#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    let rendered_paths = result
        .effects
        .helper_rendered
        .iter()
        .map(|row| row.path.clone())
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(
        have: rendered_paths,
        want: BTreeSet::from([
            "securityContexts.container".to_string(),
            "securityContexts.containers".to_string(),
            "worker.securityContexts.container".to_string(),
        ]),
        "every reachable provider candidate must survive the helper boundary: {result:#?}"
    );
}

#[test]
fn local_nil_fallback_reassignment_preserves_truthy_union() {
    let defines = DefineIndex::new();
    let context = crate::SymbolicIrContext::new(&defines);
    let source = indoc::indoc! {r#"
        {{- $enabled := .Values.feature.enabled -}}
        {{- if eq $enabled nil -}}
          {{- $enabled = ternary true false (semverCompare ">=3.0.0" .Values.version) -}}
        {{- end -}}
        {{- if $enabled -}}
        apiVersion: apps/v1
        kind: Deployment
        spec:
          replicas: {{ .Values.feature.replicas }}
        {{- end -}}
    "#};
    let ir = context.generate_contract_ir(source).finalize();
    let replicas = ir
        .uses()
        .iter()
        .find(|use_| use_.source_expr == "feature.replicas");

    assert!(
        replicas.is_some_and(|use_| {
            use_.condition.disjuncts().iter().any(|conjunction| {
                conjunction.iter().any(|predicate| {
                    let Predicate::Or(arms) = predicate else {
                        return false;
                    };
                    let has_direct_arm = arms.iter().any(|arm| {
                        arm.value_paths().contains("feature.enabled")
                            && !arm.value_paths().contains("version")
                    });
                    let has_fallback_arm = arms.iter().any(|arm| {
                        arm.value_paths().contains("feature.enabled")
                            && arm.value_paths().contains("version")
                    });
                    has_direct_arm && has_fallback_arm
                })
            }) && !use_
                .condition
                .disjuncts()
                .iter()
                .flatten()
                .any(Predicate::contains_approximation)
        }),
        "the post-assignment truth condition must retain both live arms: {ir:#?}"
    );
}

#[test]
fn helper_local_false_to_string_conversion_scopes_comparison_contract() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "feature.map" -}}
            {{- ternary "yes" "no" (eq .value "restricted") -}}
            {{- end -}}
            {{- define "feature.normalize" -}}
            {{- $mode := get .Values.feature "mode" -}}
            {{- if and (eq (kindOf $mode) "bool") (not $mode) -}}
            {{- $mode = "off" -}}
            {{- end -}}
            {{- $method := printf "feature.%s" "map" -}}
            {{- include $method (dict "value" $mode) | trim | print -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"include "feature.normalize" ."#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    let conditions = result
        .effects
        .helper_fails
        .iter()
        .filter_map(|capture| match &capture.kind {
            crate::eval_effect::CaptureKind::ComparableKind { path, schema_type }
                if path == "feature.mode" && schema_type == "string" =>
            {
                Some(capture.conjunction.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(
        have: conditions,
        want: vec![vec![Predicate::Or(vec![
            Predicate::truthy_path("feature.mode"),
            Predicate::from(Guard::TypeIs {
                path: "feature.mode".to_string(),
                schema_type: "boolean".to_string(),
            })
            .negated(),
        ])]],
        "the helper call must retain the raw-identity dispatch arm's comparison contract: {result:#?}"
    );
}

#[test]
fn chart_annotation_policy_decides_helper_output() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "common.fips.enabled" -}}
            {{- $fips := .Chart.Annotations.fips -}}
            {{- if eq "true" $fips -}}
            {{- true -}}
            {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::with_policy(
        &defines,
        crate::SymbolicPolicy {
            static_root_strings: BTreeMap::from([(
                vec![
                    "Chart".to_string(),
                    "Annotations".to_string(),
                    "fips".to_string(),
                ],
                "true".to_string(),
            )]),
            ..crate::SymbolicPolicy::default()
        },
    );
    let context = helper_context(&analysis_db);
    let mut root_bindings = analysis_db.static_root_fields().clone();
    root_bindings.insert(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    );
    let expr = single_expr(r#"include "common.fips.enabled" ."#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: Some(Predicate::True),
        "the immutable Chart annotation should decide the helper branch: {result:#?}"
    );
}

#[test]
fn nonrendering_control_regions_do_not_multiply_scalar_dispatch_states() {
    let mut defines = DefineIndex::new();
    let controls = (0..32).fold(String::new(), |mut controls, index| {
        use std::fmt::Write as _;
        let _ = write!(
            controls,
            "{{{{ with mystery .Values.feature.value{index} }}}}{{{{ end }}}}"
        );
        controls
    });
    defines.add_file_source(
        "<inline:0>",
        &format!("{{{{ define \"feature.constant\" }}}}{controls}true{{{{ end }}}}"),
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"eq (include "feature.constant" .) "true""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: Some(Predicate::True)
    );
}

#[test]
fn statically_false_inline_branch_contributes_no_helper_effects() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "feature.constant" -}}
            {{- if false -}}
            {{ .Values.dead | toJson }}
            {{- end -}}
            true
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"eq (include "feature.constant" .) "true""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    assert!(
        !result.effects.output_paths.contains("dead")
            && !result.effects.json_serialized_paths.contains("dead"),
        "the unreachable helper body must contribute no effects: {result:#?}"
    );
}

#[test]
fn helper_conditions_preserve_stringified_trimmed_local_values() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "version.is-seven" -}}
            {{- $version := .Values.image.tag | toString | trimSuffix "-jmx" -}}
            {{- if eq $version "7" -}}
            true
            {{- else -}}
            false
            {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"eq (include "version.is-seven" .) "true""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );
    let pattern = crate::helper_meta::pattern_with_lexical_escapes(
        "^7$",
        &BTreeSet::from([crate::helper_meta::LexicalEscape::TrimSuffix(
            "-jmx".to_string(),
        )]),
    );

    sim_assert_eq!(
        have: result.truth.predicate().cloned(),
        want: Some(Predicate::Or(vec![
            Predicate::from(Guard::Eq {
                path: "image.tag".to_string(),
                value: GuardValue::Int(7),
            }),
            Predicate::from(Guard::MatchesPattern {
                path: "image.tag".to_string(),
                pattern,
                templated: false,
            }),
        ])),
        "the helper branch must compare the transformed runtime value: {result:#?}"
    );
}

#[test]
fn helper_output_equality_decodes_a_versioned_boolean_dispatch() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "envoy.enabled" -}}
            {{- if not .Values.l7Proxy -}}
              {{- false -}}
            {{- else if not (kindIs "invalid" .Values.envoy.enabled) -}}
              {{- .Values.envoy.enabled -}}
            {{- else if semverCompare ">=1.16" (default "1.16" .Values.upgradeCompatibility) -}}
              {{- true -}}
            {{- else -}}
              {{- false -}}
            {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"eq (include "envoy.enabled" .) "true""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    assert!(
        result.truth.predicate().is_some(),
        "the complete helper dispatch must produce an exact equality predicate: {result:#?}"
    );
}

#[test]
fn helper_output_retains_a_token_initial_printf_argument() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "image" -}}
            {{- $repository := .Values.primary | default .Values.fallback -}}
            {{- printf "%s:tag" $repository -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"include "image" ."#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );
    let meta = result.value.as_ref().map(AbstractValue::output_meta);

    assert!(
        meta.as_ref().is_some_and(|meta| {
            meta.values()
                .any(|meta| meta.plain_slot_string_format && !meta.partial_text)
        }),
        "the helper's complete printf output must retain its token-opening argument: {result:#?}"
    );
}

#[test]
fn helper_scalar_output_retains_known_arms_beside_an_unknown_arm() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "feature.partial" -}}
            {{- if .Values.feature.explicit -}}
            true
            {{- else if mystery .Values.feature.dynamic -}}
            true
            {{- else -}}
            false
            {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"eq (include "feature.partial" .) "true""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );

    sim_assert_eq!(have: result.truth.predicate(), want: None);
    sim_assert_eq!(
        have: result.truth.when_true(),
        want: Predicate::truthy_path("feature.explicit"),
        "the exact arm remains a sound subset without making the unknown branch exhaustive"
    );
}

#[test]
fn helper_scalar_output_combines_structural_and_projected_known_arms() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "feature.mixed" -}}
            {{- if not (eq .Values.feature.explicit nil) -}}
            {{- .Values.feature.explicit -}}
            {{- else if .Values.feature.static -}}
            true
            {{- else if mystery .Values.feature.dynamic -}}
            true
            {{- else -}}
            false
            {{- end -}}
            {{- end -}}
        "#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let context = helper_context(&analysis_db);
    let root_bindings = HashMap::from([(
        "Values".to_string(),
        AbstractValue::ValuesPath(String::new()),
    )]);
    let expr = single_expr(r#"eq (include "feature.mixed" .) "true""#);
    let mut seen = HashSet::new();

    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        Some(&root_bindings),
        None,
        context,
        &mut seen,
    );
    let explicit_is_null = Predicate::from(Guard::Eq {
        path: "feature.explicit".to_string(),
        value: GuardValue::Null,
    });
    let explicit_is_true = Predicate::Or(vec![
        Predicate::from(Guard::Eq {
            path: "feature.explicit".to_string(),
            value: GuardValue::Bool(true),
        }),
        Predicate::from(Guard::MatchesPattern {
            path: "feature.explicit".to_string(),
            pattern: "^true$".to_string(),
            templated: false,
        }),
    ]);
    let want = Predicate::Or(vec![
        Predicate::all(vec![explicit_is_null.negated(), explicit_is_true]),
        Predicate::all(vec![
            explicit_is_null,
            Predicate::truthy_path("feature.static"),
        ]),
    ])
    .normalize_boolean();

    sim_assert_eq!(
        have: result.truth.when_true(),
        want: want,
        "the scalar summary must keep disjoint known arms from both semantic projections"
    );
}

#[test]
fn partial_helper_conditions_keep_typed_subsets_in_both_control_lanes() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "feature.partial" -}}
            {{- if .Values.feature.explicit -}}
            true
            {{- else if mystery .Values.feature.dynamic -}}
            true
            {{- else -}}
            false
            {{- end -}}
            {{- end -}}
        "#},
    );
    let context = crate::SymbolicIrContext::new(&defines);
    let sources = [
        (
            "structuralHost",
            indoc! {r#"
                {{- if eq (include "feature.partial" .) "true" }}
                value: {{ .Values.structuralHost.member }}
                {{- end }}
            "#},
        ),
        (
            "inlineHost",
            indoc! {r#"
                program: |-
                  {{- if eq (include "feature.partial" .) "true" }}
                  {{ .Values.inlineHost.member }}
                  {{- end }}
            "#},
        ),
    ];

    let have = sources
        .into_iter()
        .map(|(path, source)| {
            let signals = context
                .generate_contract_ir(source)
                .finalize()
                .into_schema_signals();
            signals
                .schema_evidence_by_value_path()
                .get(path)
                .and_then(|evidence| {
                    evidence.fail_implications.iter().find(|implication| {
                        matches!(
                            implication.requirements.as_slice(),
                            [helm_schema_core::FailValueRequirement::MemberHost {
                                complete_domain: false,
                                ..
                            }]
                        )
                    })
                })
                .cloned()
                .map(|implication| (path, implication))
        })
        .collect::<Vec<_>>();
    let want = ["structuralHost", "inlineHost"]
        .into_iter()
        .map(|path| {
            Some((
                path,
                helm_schema_core::ContractFailImplication {
                    outer_guards: vec![helm_schema_core::ConditionalGuard::Truthy {
                        path: "feature.explicit".to_string(),
                    }],
                    target: helm_schema_core::ContractRequirementTarget::Value,
                    requirements: vec![helm_schema_core::FailValueRequirement::MemberHost {
                        handled_kinds: Vec::new(),
                        complete_domain: false,
                    }],
                },
            ))
        })
        .collect::<Vec<_>>();

    sim_assert_eq!(
        have: have,
        want: want,
        "a partial helper condition may scope a non-owning member-host arm in either control lane"
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
        have: context_value_from_outer_expr(&expr, None, None, Some(&root_bindings), None),
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
        have: context_value_from_outer_expr(&expr, Some(&fragment_locals), None, None, None),
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
                input_identity: true,
                stringified: true,
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
        indoc! {r#"
            {{- define "select.context" -}}
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
            {{- end -}}"#},
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
    let earlier_candidate_skipped = Predicate::all(vec![
        Predicate::from(Guard::Absent {
            path: "worker.securityContexts".to_string(),
        })
        .negated(),
        Predicate::from(Guard::Absent {
            path: "worker.securityContexts.pod".to_string(),
        })
        .negated(),
        Predicate::truthy_path("worker.securityContexts.pod"),
    ])
    .negated()
    .normalize_boolean();
    assert!(
        legacy
            .meta
            .predicates
            .iter()
            .any(|branch| branch.contains(&earlier_candidate_skipped)),
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
    let exact_host_capture = BTreeSet::from([Predicate::from(Guard::Absent {
        path: "worker.securityContexts".to_string(),
    })
    .negated()]);
    let host_captures = result
        .effects
        .helper_fails
        .iter()
        .filter(|capture| {
            matches!(
                &capture.kind,
                crate::eval_effect::CaptureKind::ValueType {
                    path, schema_type, ..
                }
                    if path == "worker.securityContexts" && schema_type == "object"
            )
        })
        .collect::<Vec<_>>();
    assert!(
        host_captures.iter().any(|capture| {
            capture.conjunction.iter().cloned().collect::<BTreeSet<_>>() == exact_host_capture
        }),
        "the helper must retain the exact selected candidate's member-host obligation: \
         {host_captures:#?}"
    );
}

#[test]
fn bound_helper_continue_suppresses_the_rest_of_only_that_iteration() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "loop.values" -}}
            {{- range . -}}
              {{- if .skip -}}{{- continue -}}{{- end -}}
              {{- .payload -}}
            {{- end -}}
            {{- end -}}"#},
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
fn bound_helper_range_break_retains_scalar_candidate_selection() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "revision.limit" -}}
            {{- $result := "" -}}
            {{- range . -}}
              {{- if not (kindIs "invalid" .) -}}
                {{- $result = . -}}
                {{- break -}}
              {{- end -}}
            {{- end -}}
            {{- $result -}}
            {{- end -}}"#},
    );
    let analysis_db = IrAnalysisDb::new(&defines);
    let expr = single_expr(r#"include "revision.limit" (list .Values.primary .Values.fallback)"#);
    let context = helper_context(&analysis_db);
    let mut seen = HashSet::new();
    let result = helper_result_from_expr_with_fragment_locals(
        &expr,
        &HashMap::new(),
        None,
        None,
        context,
        &mut seen,
    );

    let invalid = |path: &str| {
        Predicate::Or(vec![
            Predicate::from(Guard::Eq {
                path: path.to_string(),
                value: GuardValue::Null,
            }),
            Predicate::from(Guard::Absent {
                path: path.to_string(),
            }),
        ])
        .normalize_boolean()
    };
    let present = |path: &str| Predicate::Not(Box::new(invalid(path))).normalize_boolean();
    let rendered_identity = |path: &str| {
        ScalarValue::Rendered(vec![ScalarRenderPart::Identity {
            path: path.to_string(),
            stringified: true,
            lexical_escapes: BTreeSet::new(),
        }])
    };
    let expected = ScalarValueDispatch {
        arms: vec![
            (
                Predicate::And(vec![invalid("fallback"), invalid("primary")]).normalize_boolean(),
                ScalarValue::Rendered(vec![ScalarRenderPart::Text(String::new())]),
            ),
            (
                Predicate::And(vec![present("fallback"), invalid("primary")]).normalize_boolean(),
                rendered_identity("fallback"),
            ),
            (present("primary"), rendered_identity("primary")),
        ],
        complete: true,
    };
    sim_assert_eq!(
        have: result.scalar_dispatch,
        want: Some(expected)
    );
}

#[test]
fn inner_range_break_does_not_exit_the_outer_range() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "nested.loop" -}}
            {{- range . -}}
              {{- range (list "only") -}}{{- break -}}{{- end -}}
              {{- .payload -}}
            {{- end -}}
            {{- end -}}"#},
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
fn bound_helper_keeps_join_observation_separate_from_output_transforms() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "prometheus.namespaces" -}}
            {{- $namespaces := list }}
            {{- if and .Values.rbac.create .Values.server.useExistingClusterRoleName }}
              {{- if .Values.server.namespaces -}}
                {{- range $ns := join "," .Values.server.namespaces | split "," }}
                  {{- $namespaces = append $namespaces (tpl $ns $) }}
                {{- end -}}
              {{- end -}}
            {{- end -}}
            {{ mustToJson $namespaces }}
            {{- end -}}"#},
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
            .helper_observed_shape_erased_paths
            .contains("server.namespaces"),
        "join's total conversion must survive the helper summary: {result:#?}",
    );
    assert!(
        !result
            .effects
            .shape_erased_paths
            .contains("server.namespaces"),
        "a body-wide observation must not transform every returned occurrence: {result:#?}",
    );

    let decoded = single_expr(r#"include "prometheus.namespaces" . | fromJsonArray"#);
    let mut seen = HashSet::new();
    let decoded = helper_result_from_expr_with_fragment_locals(
        &decoded,
        &HashMap::new(),
        None,
        None,
        context,
        &mut seen,
    );
    sim_assert_eq!(
        have: decoded.truth,
        want: TruthCondition::exact(Predicate::all(vec![
            Predicate::from(Guard::Truthy {
                path: "rbac.create".to_string(),
            }),
            Predicate::from(Guard::Truthy {
                path: "server.namespaces".to_string(),
            }),
            Predicate::from(Guard::Truthy {
                path: "server.useExistingClusterRoleName".to_string(),
            }),
        ])),
        "the decoded list is live exactly where the helper appended a namespace"
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
                input_identity: true,
                stringified: true,
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
        indoc! {r#"
            {{- define "json.roundtrip" -}}
            {{- $params := fromJson (toJson .) -}}
            {{- $doc := pick $params "doc" -}}
            {{- toJson $doc -}}
            {{- end -}}"#},
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
    let mut summary_env = EvalEnv::from_helper_context(None, None);
    summary_env.locals = locals.clone();
    let call = analysis_db.summarize_bound_helper_call(
        "json.roundtrip",
        args.get(1),
        None,
        None,
        &summary_env,
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

    let (path, json_decoded) = match doc {
        AbstractValue::JsonDecodedPath(path) => (path, true),
        AbstractValue::OutputPath(path, meta) => (path, meta.json_decoded),
        other => panic!("decoded helper output lost its path identity: {other:#?}"),
    };
    sim_assert_eq!(have: path, want: String::new());
    assert!(json_decoded, "helper output path must remain JSON-decoded");
}

#[test]
fn yaml_helper_output_preserves_structured_value_for_decoding() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:0>",
        indoc! {r#"
            {{- define "pod.template" -}}
            metadata:
              labels:
                app: test
            spec:
              hostUsers: {{ .Values.hostUsers }}
            {{- end -}}"#},
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
                input_identity: true,
                stringified: true,
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
