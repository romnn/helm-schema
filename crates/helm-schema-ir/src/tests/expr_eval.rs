use crate::abstract_value::AbstractValue;
use crate::eval_effect::{EvalResult, SelectionPolarity, SelectionTruthSource};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{
    apply_local_set_mutations_expr, bindings_for_helper_arg_with, direct_values_path, eval_expr,
    eval_exprs_effects,
};
use crate::helper_meta::HelperOutputMeta;
use crate::scalar_value::ScalarValueDispatch;
use helm_schema_ast::parse_expr_text;
use helm_schema_ast::render_printf_string_sets;
use helm_schema_ast::{TemplateExpr, parse_action_expressions};
use helm_schema_core::{Guard, GuardValue, Predicate};
use indoc::indoc;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use test_util::prelude::sim_assert_eq;

fn expr(text: &str) -> TemplateExpr {
    let exprs = parse_expr_text(text);
    sim_assert_eq!(have: exprs.len(), want: 1, "expected exactly one parsed expression");
    exprs.into_iter().next().expect("expression exists")
}

fn single_expr(action: &str) -> TemplateExpr {
    let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
    sim_assert_eq!(have: exprs.len(), want: 1, "expected exactly one parsed expression");
    exprs.into_iter().next().expect("expression exists")
}

fn direct_values_path_expr(action: &str) -> Option<String> {
    direct_values_path(&single_expr(action))
}

fn dict(entries: &[(&str, AbstractValue)]) -> AbstractValue {
    AbstractValue::Dict(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect(),
    )
}

fn env_from_root_fields(root_fields: HashMap<String, AbstractValue>) -> EvalEnv {
    EvalEnv {
        root_fields,
        allow_field_root_lookup: true,
        ..EvalEnv::default()
    }
}

#[test]
fn static_semver_fields_fold_through_decimal_printf() {
    let env = env_from_root_fields(HashMap::from([(
        "Capabilities".to_string(),
        dict(&[(
            "KubeVersion",
            dict(&[(
                "Version",
                AbstractValue::StringSet(BTreeSet::from(["v1.35.0".to_string()])),
            )]),
        )]),
    )]));

    sim_assert_eq!(
        have: eval_expr(
            &expr(
                r#"printf "%d.%d.0" (semver .Capabilities.KubeVersion.Version).Major (semver .Capabilities.KubeVersion.Version).Minor"#,
            ),
            &env,
        )
        .value,
        want: Some(AbstractValue::StringSet(BTreeSet::from([
            "1.35.0".to_string(),
        ]))),
    );
    sim_assert_eq!(
        have: eval_expr(
            &expr(
                r#"semverCompare "<1.30.0" (printf "%d.%d.0" (semver .Capabilities.KubeVersion.Version).Major (semver .Capabilities.KubeVersion.Version).Minor)"#,
            ),
            &env,
        )
        .truth
        .predicate()
        .cloned(),
        want: Some(Predicate::False),
    );
}

#[test]
fn helper_value_expression_uses_shared_expression_eval() {
    let bindings = HashMap::from([(
        "ctx".to_string(),
        AbstractValue::Dict(
            [(
                "config".to_string(),
                AbstractValue::ValuesPath("serviceAccount".to_string()),
            )]
            .into_iter()
            .collect(),
        ),
    )]);

    let env = EvalEnv::from_helper_context(Some(&bindings), None);

    sim_assert_eq!(
        have: eval_expr(&expr(".ctx.config.name | default \"x\""), &env)
            .value
            .map(|value| value.to_context_value()),
        want: Some(AbstractValue::FirstTruthy(vec![
            AbstractValue::ValuesPath("serviceAccount.name".to_string()),
            AbstractValue::StringSet(["x".to_string()].into_iter().collect()),
        ])),
    );
}

#[test]
fn helper_argument_projection_uses_shared_expression_eval() {
    let env = EvalEnv::from_helper_context(None, None);
    let bindings = bindings_for_helper_arg_with(
        Some(&expr(r#"dict "ctx" $ "config" .Values.serviceAccount"#)),
        None,
        |expr| {
            let mut result = eval_expr(expr, &env);
            result.value = result.value.map(|value| value.to_context_value());
            result
        },
    )
    .bindings;

    sim_assert_eq!(
        have: bindings,
        want: HashMap::from([
            ("ctx".to_string(), AbstractValue::RootContext),
            (
                "config".to_string(),
                AbstractValue::ValuesPath("serviceAccount".to_string()),
            ),
        ]),
    );
}

/// A map-valued `default` on a rerooted helper argument preserves the raw
/// primary's member identities. The fallback supplies the container, not
/// defaults for children selected inside the helper.
#[test]
fn defaulted_helper_root_descends_to_raw_member_paths() {
    let env = EvalEnv::from_helper_context(None, None);
    let bindings = bindings_for_helper_arg_with(
        Some(&expr(
            r#"dict "Values" (.Values.dependency | default dict)"#,
        )),
        None,
        |expr| {
            let mut result = eval_expr(expr, &env);
            result.value = result.value.map(|value| value.to_context_value());
            result
        },
    )
    .bindings;
    let selected = bindings
        .get("Values")
        .and_then(|value| value.apply_to_path(&["nameOverride".to_string()]));
    let mut expected_meta = HelperOutputMeta {
        input_identity: true,
        ..HelperOutputMeta::default()
    };
    expected_meta.conjoin_branches(&BTreeSet::from([Predicate::truthy_path("dependency")]));
    sim_assert_eq!(
        have: selected,
        want: Some(AbstractValue::OutputPath(
            "dependency.nameOverride".to_string(),
            expected_meta,
        )),
    );
}

#[test]
fn grouped_selector_preserves_scalar_identity() {
    let result = eval_expr(
        &expr("(.Values.feature).mode"),
        &EvalEnv::from_helper_context(None, None),
    );

    sim_assert_eq!(
        have: result.scalar_dispatch,
        want: Some(ScalarValueDispatch::identity(
            "feature.mode"
        )),
    );
}

#[test]
fn bound_path_resolution_uses_shared_expression_eval() {
    let bindings = HashMap::from([(
        "config".to_string(),
        AbstractValue::ValuesPath("serviceAccount".to_string()),
    )]);

    let env = EvalEnv::from_helper_context(Some(&bindings), None);
    let path = eval_expr(&expr(".config.name"), &env)
        .value
        .as_ref()
        .and_then(AbstractValue::unique_path);

    sim_assert_eq!(have: path, want: Some("serviceAccount.name".to_string()));
}

#[test]
fn direct_root_values_path_is_an_expression_eval_projection() {
    sim_assert_eq!(
        have: direct_values_path_expr(".Values.foo.bar"),
        want: Some("foo.bar".to_string())
    );
    sim_assert_eq!(
        have: direct_values_path_expr("$.Values.X"),
        want: Some("X".to_string())
    );
    sim_assert_eq!(
        have: direct_values_path_expr("$root.Values.Y"),
        want: Some("Y".to_string())
    );
    sim_assert_eq!(
        have: direct_values_path_expr("((.Values.appVersions).airtype).global"),
        want: Some("appVersions.airtype.global".to_string())
    );
}

#[test]
fn direct_values_path_projection_rejects_computed_and_contextual_paths() {
    sim_assert_eq!(have: direct_values_path_expr(".context.Values.X"), want: None);
    sim_assert_eq!(
        have: direct_values_path_expr(r#"eq .Values.X ".Values.fake""#),
        want: None
    );
    sim_assert_eq!(
        have: direct_values_path_expr(r#"" .Values.fake ""#),
        want: None
    );
}

#[test]
fn set_default_chart_paths_ignores_unrelated_default_inside_set_rhs() {
    let exprs = parse_expr_text(
        r#"$_ := set .serviceAccount "name" (printf "%s" (.other | default "fallback"))"#,
    );
    let env = EvalEnv::from_helper_context(None, Some(&AbstractValue::ValuesPath(String::new())));

    sim_assert_eq!(
        have: eval_exprs_effects(&exprs, &env).chart_default_paths,
        want: BTreeSet::new(),
    );
}

#[test]
fn string_transform_pipeline_preserves_all_printf_argument_paths() {
    let expr = single_expr(r#"printf "%s-%s" .Values.primary.name .Values.suffix | trunc 63"#);
    let result = eval_expr(&expr, &EvalEnv::default());

    for path in ["primary.name", "suffix"] {
        assert!(
            result.effects.derived_text_paths.contains(path),
            "{path} should remain visible through printf as derived text"
        );
        // trunc consumes printf's derived text, so it must not bind a
        // string contract on the raw argument paths: printf renders
        // anything.
        sim_assert_eq!(
            have: result.effects.type_hints.get(path),
            want: None,
            "{path} must not inherit trunc's contract through printf"
        );
    }
}

#[test]
fn quote_pipeline_erases_input_shape_without_typing() {
    let result = eval_expr(&single_expr(r".Values.flag | quote"), &EvalEnv::default());

    sim_assert_eq!(
        have: result.effects.type_hints.get("flag"),
        want: None,
        "quote renders any input through strval, so it types nothing"
    );
    assert!(
        result.effects.shape_erased_paths.contains("flag"),
        "the sink observes quote's rendered text, never the input shape"
    );
}

#[test]
fn local_fragment_variable_effects_include_shallow_source_paths() {
    let mut env = EvalEnv::default();
    env.locals.insert(
        "nodeSelector".to_string(),
        AbstractValue::Choice(
            [
                AbstractValue::ValuesPath("global.nodeSelector".to_string()),
                AbstractValue::ValuesPath("nodeSelector".to_string()),
            ]
            .into_iter()
            .collect(),
        ),
    );

    let result = eval_expr(&single_expr("$nodeSelector"), &env);

    sim_assert_eq!(
        have: result.effects.local_source_paths,
        want: BTreeSet::from([
            "global.nodeSelector".to_string(),
            "nodeSelector".to_string(),
        ]),
    );
}

#[test]
fn printf_exact_rendering_only_accepts_supported_string_formats() {
    let values = [BTreeSet::from(["x".to_string()])];

    sim_assert_eq!(
        have: render_printf_string_sets("prefix-%s-%%", &values),
        want: Some(BTreeSet::from(["prefix-x-%".to_string()]))
    );
    sim_assert_eq!(have: render_printf_string_sets("%d", &values), want: None);
    sim_assert_eq!(
        have: render_printf_string_sets("literal", &[BTreeSet::from(["unused".to_string()])]),
        want: None
    );
    sim_assert_eq!(have: render_printf_string_sets("%s-%s", &values), want: None);
}

#[test]
fn integer_index_on_values_path_descends_array_item_wildcard() {
    let expr = single_expr(r"index .Values.sentinel.externalAccess.service.loadBalancerIP 0");
    let result = eval_expr(&expr, &EvalEnv::default());

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::ValuesPath(
            "sentinel.externalAccess.service.loadBalancerIP.*".to_string()
        ))
    );
    assert!(
        result
            .effects
            .output_paths
            .contains("sentinel.externalAccess.service.loadBalancerIP.*")
    );
}

#[test]
fn integer_index_on_known_list_stays_positional() {
    let expr = single_expr(r#"index (list "root" "scope" "pod") 1"#);
    let result = eval_expr(&expr, &EvalEnv::default());

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::StringSet(BTreeSet::from([
            "scope".to_string()
        ])))
    );
}

#[test]
fn get_requires_its_values_backed_host_to_be_an_object() {
    let mut env = EvalEnv::default();
    env.locals.insert(
        "context".to_string(),
        AbstractValue::JsonDecodedPath("contexts.*".to_string()),
    );
    let result = eval_expr(&single_expr(r#"get $context "creds""#), &env);

    assert!(result.effects.helper_fails.iter().any(|capture| {
        matches!(
            capture.kind,
            crate::eval_effect::CaptureKind::MemberAccess { .. }
        ) && capture.conjunction
            == vec![
                Predicate::from(Guard::TypeIs {
                    path: "contexts.*".to_string(),
                    schema_type: "object".to_string(),
                })
                .negated(),
            ]
    }));
}

#[test]
fn grouped_selector_requires_an_object_only_when_receiver_is_present() {
    let result = eval_expr(
        &single_expr(r"(.Values.resources.limits).memory"),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::ValuesPath(
            "resources.limits.memory".to_string()
        )),
    );
    let target_capture = result
        .effects
        .helper_fails
        .iter()
        .find(|capture| {
            matches!(
                capture.kind,
                crate::eval_effect::CaptureKind::MemberAccess { .. }
            ) && capture.conjunction.iter().any(|predicate| {
                matches!(
                    predicate,
                    Predicate::Not(inner)
                        if matches!(
                            inner.as_ref(),
                            Predicate::Guard(Guard::TypeIs { path, schema_type })
                                if path == "resources.limits" && schema_type == "object"
                        )
                )
            })
        })
        .expect("grouped receiver member-host capture");
    sim_assert_eq!(
        have: &target_capture.conjunction,
        want: &vec![
            Predicate::Not(Box::new(Predicate::Guard(Guard::Absent {
                path: "resources.limits".to_string(),
            }))),
            Predicate::Not(Box::new(Predicate::Guard(Guard::TypeIs {
                path: "resources.limits".to_string(),
                schema_type: "object".to_string(),
            }))),
        ],
    );
}

#[test]
fn ungrouped_selector_still_requires_the_intermediate_member() {
    let result = eval_expr(
        &single_expr(r".Values.resources.limits.memory"),
        &EvalEnv::default(),
    );

    assert!(result.effects.helper_fails.iter().any(|capture| {
        matches!(
            capture.kind,
            crate::eval_effect::CaptureKind::MemberAccess { .. }
        ) && capture.conjunction
            == vec![Predicate::Not(Box::new(Predicate::Guard(Guard::TypeIs {
                path: "resources.limits".to_string(),
                schema_type: "object".to_string(),
            })))]
    }));
}

#[test]
fn set_call_updates_local_key_with_assigned_literal() {
    let expr = single_expr(r#"set $config (printf "%s" "name") "generated""#);
    let mut env = EvalEnv::default();
    env.locals.insert(
        "config".to_string(),
        dict(&[
            (
                "name",
                AbstractValue::ValuesPath("serviceAccount.name".to_string()),
            ),
            (
                "annotations",
                AbstractValue::ValuesPath("serviceAccount.annotations".to_string()),
            ),
        ]),
    );

    assert!(apply_local_set_mutations_expr(&expr, &mut env));

    sim_assert_eq!(
        have: env.locals.get("config"),
        want: Some(&AbstractValue::Overlay {
            entries: BTreeMap::from([(
                "name".to_string(),
                AbstractValue::StringSet(BTreeSet::from(["generated".to_string()])),
            )]),
            fallback: Box::new(dict(&[
                (
                    "name",
                    AbstractValue::ValuesPath("serviceAccount.name".to_string())
                ),
                (
                    "annotations",
                    AbstractValue::ValuesPath("serviceAccount.annotations".to_string()),
                ),
            ])),
        })
    );
}

#[test]
fn set_call_inside_throwaway_assignment_updates_local_key() {
    let expr = single_expr(r#"$_ := set $config (printf "%s" "name") "generated""#);
    let mut env = EvalEnv::default();
    env.locals.insert(
        "config".to_string(),
        dict(&[(
            "name",
            AbstractValue::ValuesPath("serviceAccount.name".to_string()),
        )]),
    );

    assert!(apply_local_set_mutations_expr(&expr, &mut env));

    sim_assert_eq!(
        have: env.locals.get("config"),
        want: Some(&AbstractValue::Overlay {
            entries: BTreeMap::from([(
                "name".to_string(),
                AbstractValue::StringSet(BTreeSet::from(["generated".to_string()])),
            )]),
            fallback: Box::new(dict(&[(
                "name",
                AbstractValue::ValuesPath("serviceAccount.name".to_string()),
            )])),
        })
    );
}

#[test]
fn set_call_preserves_assigned_value_path() {
    let expr = single_expr(r#"$_ := set $config "name" .Values.generatedName"#);
    let mut env = EvalEnv::default();
    env.locals.insert(
        "config".to_string(),
        dict(&[(
            "name",
            AbstractValue::ValuesPath("serviceAccount.name".to_string()),
        )]),
    );

    assert!(apply_local_set_mutations_expr(&expr, &mut env));

    let result = eval_expr(&single_expr(r"$config.name"), &env);
    sim_assert_eq!(
        have: result.effects.output_paths,
        want: BTreeSet::from(["generatedName".to_string()])
    );
}

#[test]
fn selector_on_local_dict_records_only_selected_child_reads() {
    let expr = single_expr(r"$config.annotations");
    let mut env = EvalEnv::default();
    env.locals.insert(
        "config".to_string(),
        dict(&[
            (
                "name",
                AbstractValue::ValuesPath("serviceAccount.name".to_string()),
            ),
            (
                "annotations",
                AbstractValue::ValuesPath("serviceAccount.annotations".to_string()),
            ),
        ]),
    );

    let result = eval_expr(&expr, &env);

    sim_assert_eq!(
        have: result.effects.output_paths,
        want: BTreeSet::from(["serviceAccount.annotations".to_string()])
    );
}

#[test]
fn unsupported_printf_format_types_nothing_without_exact_string() {
    let expr = single_expr(r#"printf "%d" .Values.count"#);
    let result = eval_expr(&expr, &EvalEnv::default());

    sim_assert_eq!(
        have: result.effects.type_hints.get("count"),
        want: None,
        "Go fmt embeds verb mismatches in the output instead of failing, so printf types nothing"
    );
    assert!(
        result.effects.derived_text_paths.contains("count"),
        "printf arguments stay visible as derived text"
    );
    assert!(
        result
            .value
            .as_ref()
            .map(AbstractValue::strings)
            .unwrap_or_default()
            .is_empty(),
        "unsupported printf formats must not synthesize exact strings"
    );
}

#[test]
fn pipeline_ternary_returns_value_branches_not_condition() {
    let expr = single_expr(
        r#"typeIs "string" .Values.config | ternary .Values.config (.Values.config | toYaml)"#,
    );
    let result = eval_expr(&expr, &EvalEnv::default());

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::ValuesPath("config".to_string()))
    );
    sim_assert_eq!(
        have: result.effects.type_hints.get("config"),
        want: None,
        "a type test selects an output arm; it does not restrict the input domain"
    );
    sim_assert_eq!(
        have: result.effects.guarded_type_hints.get("config"),
        want: Some(&BTreeSet::from(["string".to_string()])),
        "the tested arm remains an accepted output alternative"
    );
}

#[test]
fn type_test_uses_the_structural_value_instead_of_its_influences() {
    let env = EvalEnv {
        locals: HashMap::from([(
            "obj".to_string(),
            AbstractValue::Dict(BTreeMap::from([
                (
                    "merge".to_string(),
                    AbstractValue::ValuesPath("service.merge".to_string()),
                ),
                (
                    "patch".to_string(),
                    AbstractValue::ValuesPath("service.patch".to_string()),
                ),
            ])),
        )]),
        ..EvalEnv::default()
    };

    let result = eval_expr(&expr(r#"kindIs "map" $obj"#), &env);

    sim_assert_eq!(
        have: result.truth.predicate(),
        want: Some(&Predicate::True),
        "the dict is a map regardless of the runtime kinds of its member values"
    );
}

#[test]
fn invalid_kind_requires_one_exact_subject_identity() {
    let result = eval_expr(
        &expr(r#"kindIs "invalid" (ternary .Values.value "fallback" .Values.enabled)"#),
        &EvalEnv::default(),
    );

    sim_assert_eq!(have: result.truth.predicate(), want: None);
}

#[test]
fn invalid_kind_abstains_for_a_meta_selected_subject_identity() {
    let mut metadata = HelperOutputMeta {
        input_identity: true,
        ..HelperOutputMeta::default()
    };
    metadata.conjoin_branches(&BTreeSet::from([Predicate::truthy_path("enabled")]));
    let env = EvalEnv {
        locals: HashMap::from([(
            "selected".to_string(),
            AbstractValue::ValuesPath("value".to_string()),
        )]),
        local_output_meta: HashMap::from([(
            "selected".to_string(),
            BTreeMap::from([("value".to_string(), metadata)]),
        )]),
        ..EvalEnv::default()
    };

    let result = eval_expr(&expr(r#"kindIs "invalid" $selected"#), &env);

    sim_assert_eq!(have: result.truth.predicate(), want: None);
}

#[test]
fn invalid_kind_abstains_for_a_default_selected_subject_identity() {
    let env = EvalEnv {
        locals: HashMap::from([(
            "selected".to_string(),
            AbstractValue::ValuesPath("value".to_string()),
        )]),
        local_default_paths: HashMap::from([(
            "selected".to_string(),
            BTreeSet::from(["value".to_string()]),
        )]),
        ..EvalEnv::default()
    };

    let result = eval_expr(&expr(r#"kindIs "invalid" $selected"#), &env);

    sim_assert_eq!(have: result.truth.predicate(), want: None);
}

#[test]
fn go_regex_literal_escaping_leaves_re2_hyphens_bare() {
    sim_assert_eq!(
        have: crate::escape_regex_literal("prefix-with.+symbols"),
        want: r"prefix-with\.\+symbols"
    );
}

#[test]
fn lexical_escape_patterns_use_the_shared_go_regex_quoting() {
    let pattern = crate::helper_meta::pattern_with_lexical_escapes(
        "^value$",
        &BTreeSet::from([crate::helper_meta::LexicalEscape::TrimSuffix(
            "-/x.y".to_string(),
        )]),
    );

    sim_assert_eq!(have: pattern, want: r"^(?:value)(?:-/x\.y)?$");
}

#[test]
fn quoted_negative_zero_keeps_falsy_pattern_truth_unknown() {
    let result = eval_expr(
        &expr(r#"contains "-" (quote .Values.value)"#),
        &EvalEnv::default(),
    );

    sim_assert_eq!(have: result.truth.predicate(), want: None);
}

#[test]
fn values_numeric_type_spellings_remain_provenance_dependent() {
    let int64 = eval_expr(
        &expr(r#"typeIs "int64" .Values.value"#),
        &EvalEnv::default(),
    );
    let float64 = eval_expr(
        &expr(r#"kindIs "float64" .Values.value"#),
        &EvalEnv::default(),
    );
    let integer = Predicate::from(Guard::TypeIs {
        path: "value".to_string(),
        schema_type: "integer".to_string(),
    });
    let number = Predicate::from(Guard::TypeIs {
        path: "value".to_string(),
        schema_type: "number".to_string(),
    });

    sim_assert_eq!(have: int64.truth.predicate(), want: None);
    sim_assert_eq!(have: int64.truth.when_true(), want: Predicate::False);
    sim_assert_eq!(have: int64.truth.when_false(), want: integer.negated());
    sim_assert_eq!(have: float64.truth.predicate(), want: None);
    sim_assert_eq!(
        have: float64.truth.when_true(),
        want: Predicate::all(vec![
            number.clone(),
            Predicate::from(Guard::TypeIs {
                path: "value".to_string(),
                schema_type: "integer".to_string(),
            })
            .negated(),
        ])
        .normalize_boolean()
    );
    sim_assert_eq!(have: float64.truth.when_false(), want: number.negated());
}

#[test]
fn stringified_pattern_truth_keeps_raw_string_subsets() {
    let result = eval_expr(
        &expr(r#"regexMatch "^[0-9]+$" (toString .Values.value)"#),
        &EvalEnv::default(),
    );

    sim_assert_eq!(have: result.truth.predicate(), want: None);
    sim_assert_eq!(
        have: result.truth.when_true(),
        want: Predicate::from(Guard::MatchesPattern {
            path: "value".to_string(),
            pattern: "^[0-9]+$".to_string(),
            templated: false,
        })
    );
    sim_assert_eq!(
        have: result.truth.when_false(),
        want: Predicate::from(Guard::NotMatchesPattern {
            path: "value".to_string(),
            pattern: "^[0-9]+$".to_string(),
        })
    );
}

#[test]
fn stringified_contains_uses_go_compatible_literal_pattern_syntax() {
    let result = eval_expr(
        &expr(r#"contains "foo-bar" (toString .Values.value)"#),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result.truth.when_true(),
        want: Predicate::from(Guard::MatchesPattern {
            path: "value".to_string(),
            pattern: "foo-bar".to_string(),
            templated: false,
        })
    );
    sim_assert_eq!(
        have: result.truth.when_false(),
        want: Predicate::from(Guard::NotMatchesPattern {
            path: "value".to_string(),
            pattern: "foo-bar".to_string(),
        })
    );
}

#[test]
fn ternary_preserves_scalar_branch_dispatch() {
    let expr = single_expr(r#"ternary true false (semverCompare ">=3.0.0" .Values.version)"#);
    let result = eval_expr(&expr, &EvalEnv::default());

    assert!(
        result.scalar_dispatch.as_ref().is_some_and(|dispatch| {
            dispatch.complete
                && dispatch.arms.len() == 2
                && dispatch
                    .arms
                    .iter()
                    .all(|(condition, _)| !condition.contains_approximation())
        }),
        "the selected Boolean literals must remain an exact scalar dispatch: {result:#?}"
    );
    assert!(
        result
            .truth
            .predicate()
            .is_some_and(|predicate| predicate.value_paths().contains("version")),
        "the ternary result must retain its selector condition: {result:#?}"
    );
}

#[test]
fn base64_pipeline_preserves_source_path() {
    let expr = single_expr(r".Values.auth.password | toString | b64enc");
    let result = eval_expr(&expr, &EvalEnv::default());

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::ValuesPath("auth.password".to_string()))
    );
}

#[test]
fn uniq_pipeline_preserves_local_list_items() {
    let expr = single_expr(r"$pullSecrets | uniq");
    let mut env = EvalEnv::default();
    env.locals.insert(
        "pullSecrets".to_string(),
        AbstractValue::List(vec![AbstractValue::ValuesPath(
            "image.pullSecrets.*".to_string(),
        )]),
    );
    let result = eval_expr(&expr, &env);

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::List(vec![AbstractValue::ValuesPath(
            "image.pullSecrets.*".to_string(),
        )]))
    );
}

#[test]
fn split_list_preserves_exact_segment_sequence() {
    let expr = single_expr(r#"splitList "." "auth.password""#);
    let result = eval_expr(&expr, &EvalEnv::default());

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::List(vec![
            AbstractValue::StringSet(BTreeSet::from(["auth".to_string()])),
            AbstractValue::StringSet(BTreeSet::from(["password".to_string()])),
        ]))
    );
}

#[test]
fn split_list_preserves_mixed_length_path_alternatives() {
    let expr = single_expr(r#"splitList "." (coalesce "auth.password" "global.auth.password")"#);
    let result = eval_expr(&expr, &EvalEnv::default());

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::Choice(BTreeSet::from([
            AbstractValue::List(vec![
                AbstractValue::StringSet(BTreeSet::from(["auth".to_string()])),
                AbstractValue::StringSet(BTreeSet::from(["password".to_string()])),
            ]),
            AbstractValue::List(vec![
                AbstractValue::StringSet(BTreeSet::from(["global".to_string()])),
                AbstractValue::StringSet(BTreeSet::from(["auth".to_string()])),
                AbstractValue::StringSet(BTreeSet::from(["password".to_string()])),
            ]),
        ])))
    );
}

#[test]
fn append_projects_the_source_collection_to_its_member_domain() {
    let result = eval_expr(
        &single_expr(r#"append (default (list) .Values.items) "synthetic""#),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::List(vec![
            AbstractValue::ValuesPath("items.*".to_string()),
            AbstractValue::StringSet(BTreeSet::from(["synthetic".to_string()])),
        ])),
    );
}

#[test]
fn first_and_reverse_preserve_list_structure() {
    let first = eval_expr(&single_expr(r#"first (list "a" "b")"#), &EvalEnv::default());
    sim_assert_eq!(
        have: first.value,
        want: Some(AbstractValue::StringSet(BTreeSet::from(["a".to_string()])))
    );
    let piped_first = eval_expr(
        &single_expr(r#"(list "a" "b") | first"#),
        &EvalEnv::default(),
    );
    sim_assert_eq!(
        have: piped_first.value,
        want: Some(AbstractValue::StringSet(BTreeSet::from(["a".to_string()])))
    );

    let reverse = eval_expr(
        &single_expr(r#"reverse (list "a" "b")"#),
        &EvalEnv::default(),
    );
    sim_assert_eq!(
        have: reverse.value,
        want: Some(AbstractValue::List(vec![
            AbstractValue::StringSet(BTreeSet::from(["b".to_string()])),
            AbstractValue::StringSet(BTreeSet::from(["a".to_string()])),
        ]))
    );
    let piped_reverse = eval_expr(
        &single_expr(r#"(list "a" "b") | reverse"#),
        &EvalEnv::default(),
    );
    sim_assert_eq!(
        have: piped_reverse.value,
        want: Some(AbstractValue::List(vec![
            AbstractValue::StringSet(BTreeSet::from(["b".to_string()])),
            AbstractValue::StringSet(BTreeSet::from(["a".to_string()])),
        ]))
    );
}

#[test]
fn strict_pipeline_calls_match_direct_operand_contracts() {
    for (direct, pipeline) in [
        ("len .Values.input", ".Values.input | len"),
        ("first .Values.input", ".Values.input | first"),
        ("reverse .Values.input", ".Values.input | reverse"),
        (
            r#"eq .Values.input "active""#,
            r#".Values.input | eq "active""#,
        ),
        (
            r#"ne .Values.input "active""#,
            r#".Values.input | ne "active""#,
        ),
    ] {
        let direct = eval_expr(&single_expr(direct), &EvalEnv::default());
        let pipeline_result = eval_expr(&single_expr(pipeline), &EvalEnv::default());

        assert!(
            !direct.effects.helper_fails.is_empty(),
            "the direct call should establish the reference contract: {pipeline}"
        );
        sim_assert_eq!(
            have: pipeline_result.effects.helper_fails,
            want: direct.effects.helper_fails,
            "the pipeline form must preserve the direct call's runtime contract: {pipeline}"
        );
    }
}

#[test]
fn migrated_invocation_families_match_between_call_and_pipeline_syntax() {
    for (direct, pipeline) in [
        ("first .Values.input", ".Values.input | first"),
        (
            r#"eq .Values.input "active""#,
            r#".Values.input | eq "active""#,
        ),
        (
            r#"ternary "yes" "no" .Values.input"#,
            r#".Values.input | ternary "yes" "no""#,
        ),
        (
            r#"replace "old" "new" .Values.input"#,
            r#".Values.input | replace "old" "new""#,
        ),
        (
            r#"trimSuffix "x" .Values.input"#,
            r#".Values.input | trimSuffix "x""#,
        ),
        ("fromJson .Values.input", ".Values.input | fromJson"),
        (r#"join "," .Values.input"#, r#".Values.input | join ",""#),
        (r#"split "," .Values.input"#, r#".Values.input | split ",""#),
        ("b64enc .Values.input", ".Values.input | b64enc"),
    ] {
        let direct_result = eval_expr(&single_expr(direct), &EvalEnv::default());
        let pipeline_result = eval_expr(&single_expr(pipeline), &EvalEnv::default());

        sim_assert_eq!(
            have: pipeline_result,
            want: direct_result,
            "invocation semantics diverged for {pipeline}"
        );
    }
}

#[test]
fn integer_and_float_comparisons_keep_distinct_runtime_kinds() {
    let captures = |action: &str| {
        eval_expr(&single_expr(action), &EvalEnv::default())
            .effects
            .helper_fails
            .into_iter()
            .map(|capture| (capture.kind, capture.conjunction))
            .collect::<BTreeSet<_>>()
    };

    sim_assert_eq!(
        have: captures("eq .Values.input 1"),
        want: BTreeSet::from([(
            crate::eval_effect::CaptureKind::ComparableKind {
                path: "input".to_string(),
                schema_type: "integer".to_string(),
            },
            Vec::new(),
        )])
    );
    sim_assert_eq!(
        have: captures("eq .Values.input 1.5"),
        want: BTreeSet::from([(
            crate::eval_effect::CaptureKind::ComparableKind {
                path: "input".to_string(),
                schema_type: "number".to_string(),
            },
            Vec::new(),
        )])
    );
}

#[test]
fn helper_argument_fields_resolve_from_dot_root() {
    let expr = single_expr(r#"default "generated" .config.name"#);
    let env = env_from_root_fields(HashMap::from([(
        "config".to_string(),
        AbstractValue::ValuesPath("serviceAccount".to_string()),
    )]));

    let result = eval_expr(&expr, &env);

    assert!(
        result.effects.defaults.contains("serviceAccount.name"),
        "default should attach to the values path reached through .config.name"
    );
}

#[test]
fn default_choice_records_primary_and_fallback_selection_conditions() {
    let result = eval_expr(
        &single_expr("default .Values.persistence.storageClass .Values.global.storageClass"),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result
            .effects
            .local_output_meta
            .get("global.storageClass")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("global.storageClass"),
        ])])),
    );
    sim_assert_eq!(
        have: result
            .effects
            .local_output_meta
            .get("persistence.storageClass")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("global.storageClass").negated(),
        ])])),
    );
}

#[test]
fn chained_default_records_the_composed_primary_selection_on_the_final_fallback() {
    let result = eval_expr(
        &single_expr(".Values.x | default .Values.y | default .Values.z"),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result
            .effects
            .local_output_meta
            .get("z")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("x").negated(),
            Predicate::truthy_path("y").negated(),
        ])])),
    );
}

#[test]
fn opaque_default_primary_records_an_unlowerable_fallback_selection() {
    let result = eval_expr(
        &single_expr(r#"printf "%q" .Values.alpha | default .Values.omega"#),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result
            .effects
            .local_output_meta
            .get("omega")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::approximate_output_selection(
                "default fallback after opaque primary",
                BTreeSet::from(["alpha".to_string()]),
                Predicate::False,
            ),
        ])])),
    );
}

#[test]
fn formatter_default_chain_uses_rendered_truthiness_for_the_final_fallback() {
    let result = eval_expr(
        &single_expr(r#"printf "%s" .Values.alpha | default .Values.beta | default .Values.omega"#),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result
            .effects
            .local_output_meta
            .get("omega")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::from(Guard::MatchesPattern {
                path: "alpha".to_string(),
                pattern: "^$".to_string(),
                templated: false,
            }),
            Predicate::truthy_path("beta").negated(),
        ])])),
    );
}

#[test]
fn generated_default_chain_keeps_truthy_primary_string_consumption() {
    let result = eval_expr(
        &single_expr(
            r#".Values.secret | default (include "existing" .) | default (randAlphaNum 16) | b64enc"#,
        ),
        &EvalEnv::default(),
    );

    assert!(
        result.effects.helper_fails.iter().any(|capture| {
            matches!(
                &capture.kind,
                crate::eval_effect::CaptureKind::ValueType {
                    path,
                    schema_type,
                    ..
                } if path == "secret" && schema_type == "string"
            ) && capture
                .conjunction
                .contains(&Predicate::truthy_path("secret"))
        }),
        "a truthy raw primary reaches the strict b64enc consumer: {result:#?}"
    );
}

#[test]
fn literal_and_string_set_default_primaries_record_exact_fallback_reachability() {
    for (expression, predicates) in [
        (r#""" | default .Values.omega"#, Some(BTreeSet::new())),
        (r#""x" | default .Values.omega"#, None),
        (
            r#"ternary "" "" .Values.choose | default .Values.omega"#,
            Some(BTreeSet::new()),
        ),
        (
            r#"ternary "x" "y" .Values.choose | default .Values.omega"#,
            None,
        ),
    ] {
        let result = eval_expr(&single_expr(expression), &EvalEnv::default());
        sim_assert_eq!(
            have: result
                .effects
                .local_output_meta
                .get("omega")
                .map(|meta| &meta.predicates),
            want: predicates.as_ref(),
            "literal primary selection mismatch for {expression}"
        );
    }
}

#[test]
fn coalesce_records_ordered_candidate_selection_conditions() {
    let result = eval_expr(
        &single_expr("coalesce .Values.primary .Values.fallback .Values.last"),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result
            .effects
            .local_output_meta
            .get("primary")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("primary"),
        ])])),
    );
    sim_assert_eq!(
        have: result
            .effects
            .local_output_meta
            .get("fallback")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("primary").negated(),
            Predicate::truthy_path("fallback"),
        ])])),
    );
    sim_assert_eq!(
        have: result
            .effects
            .local_output_meta
            .get("last")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("primary").negated(),
            Predicate::truthy_path("fallback").negated(),
            Predicate::truthy_path("last"),
        ])])),
    );

    let consumed = eval_expr(
        &single_expr("coalesce .Values.primary .Values.fallback | b64enc"),
        &EvalEnv::default(),
    );
    let failure_captures = consumed
        .effects
        .helper_fails
        .into_iter()
        .map(|capture| {
            (
                capture.kind,
                capture.conjunction.into_iter().collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(
        have: failure_captures,
        want: BTreeSet::from([
            (
                crate::eval_effect::CaptureKind::ValueType {
                    path: "primary".to_string(),
                    schema_type: "string".to_string(),
                    null_aborts: false,
                },
                BTreeSet::from([Predicate::truthy_path("primary")]),
            ),
            (
                crate::eval_effect::CaptureKind::ValueType {
                    path: "fallback".to_string(),
                    schema_type: "string".to_string(),
                    null_aborts: false,
                },
                BTreeSet::from([
                    Predicate::truthy_path("primary").negated(),
                    Predicate::truthy_path("fallback"),
                ]),
            ),
        ])
    );
}

#[test]
fn short_circuit_calls_return_guarded_operand_values() {
    let or_result = eval_expr(
        &single_expr("or .Values.primary .Values.fallback .Values.last"),
        &EvalEnv::default(),
    );
    sim_assert_eq!(
        have: or_result.value,
        want: Some(AbstractValue::Choice(BTreeSet::from([
            AbstractValue::ValuesPath("fallback".to_string()),
            AbstractValue::ValuesPath("last".to_string()),
            AbstractValue::ValuesPath("primary".to_string()),
        ]))),
    );
    sim_assert_eq!(
        have: or_result
            .effects
            .local_output_meta
            .get("fallback")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("primary").negated(),
            Predicate::truthy_path("fallback"),
        ])])),
    );
    sim_assert_eq!(
        have: or_result
            .effects
            .local_output_meta
            .get("last")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("primary").negated(),
            Predicate::truthy_path("fallback").negated(),
        ])])),
    );

    let and_result = eval_expr(
        &single_expr("and .Values.primary .Values.fallback .Values.last"),
        &EvalEnv::default(),
    );
    sim_assert_eq!(
        have: and_result
            .effects
            .local_output_meta
            .get("fallback")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("primary"),
            Predicate::truthy_path("fallback").negated(),
        ])])),
    );
    sim_assert_eq!(
        have: and_result
            .effects
            .local_output_meta
            .get("last")
            .map(|meta| &meta.predicates),
        want: Some(&BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("primary"),
            Predicate::truthy_path("fallback"),
        ])])),
    );
}

#[test]
fn short_circuit_calls_scope_later_runtime_failures_to_execution() {
    let result = eval_expr(
        &single_expr("or .Values.ready (b64enc .Values.payload)"),
        &EvalEnv::default(),
    );
    let failure_captures = result
        .effects
        .helper_fails
        .into_iter()
        .map(|capture| {
            (
                capture.kind,
                capture.conjunction.into_iter().collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeSet<_>>();

    sim_assert_eq!(
        have: failure_captures,
        want: BTreeSet::from([(
            crate::eval_effect::CaptureKind::ValueType {
                path: "payload".to_string(),
                schema_type: "string".to_string(),
                null_aborts: false,
            },
            BTreeSet::from([Predicate::truthy_path("ready").negated()]),
        )]),
    );
}

#[test]
fn truth_only_locals_drive_short_circuit_execution() {
    let mut env = EvalEnv::default();
    env.local_truthy_reductions
        .insert("shouldContinue".to_string(), Predicate::True);

    let result = eval_expr(
        &single_expr(r#"and $shouldContinue (hasKey .Values.cfg "a")"#),
        &env,
    );
    let failure_captures = result
        .effects
        .helper_fails
        .into_iter()
        .map(|capture| {
            (
                capture.kind,
                capture.conjunction.into_iter().collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeSet<_>>();

    sim_assert_eq!(
        have: failure_captures,
        want: BTreeSet::from([
            (
                crate::eval_effect::CaptureKind::ValueType {
                    path: "cfg".to_string(),
                    schema_type: "object".to_string(),
                    null_aborts: true,
                },
                BTreeSet::new(),
            ),
            (
                crate::eval_effect::CaptureKind::AbsenceAborts {
                    path: "cfg".to_string(),
                },
                BTreeSet::new(),
            ),
        ]),
    );
}

#[test]
fn unset_nil_behavior_distinguishes_direct_access_from_a_local_binding() {
    let direct = eval_expr(
        &single_expr(r#"unset .Values.absent "k""#),
        &EvalEnv::default(),
    );
    let direct_failures = direct
        .effects
        .helper_fails
        .into_iter()
        .map(|capture| capture.kind)
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(
        have: direct_failures,
        want: BTreeSet::from([
            crate::eval_effect::CaptureKind::ValueType {
                path: "absent".to_string(),
                schema_type: "object".to_string(),
                null_aborts: true,
            },
            crate::eval_effect::CaptureKind::AbsenceAborts {
                path: "absent".to_string(),
            },
        ]),
    );

    let mut env = EvalEnv::default();
    env.locals.insert(
        "x".to_string(),
        AbstractValue::ValuesPath("absent".to_string()),
    );
    let local = eval_expr(&single_expr(r#"unset $x "k""#), &env);
    let local_failures = local
        .effects
        .helper_fails
        .into_iter()
        .map(|capture| capture.kind)
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(
        have: local_failures,
        want: BTreeSet::from([crate::eval_effect::CaptureKind::ValueType {
            path: "absent".to_string(),
            schema_type: "object".to_string(),
            null_aborts: false,
        }]),
    );
}

#[test]
fn local_selector_truth_comes_from_the_selected_value() {
    let mut env = EvalEnv::default();
    env.locals.insert(
        "plugin".to_string(),
        AbstractValue::ValuesPath("plugins.*".to_string()),
    );
    env.local_truthy_reductions
        .insert("plugin".to_string(), Predicate::truthy_path("plugins.*"));

    let result = eval_expr(&single_expr("$plugin.hostPath"), &env);

    sim_assert_eq!(
        have: result.truth.predicate(),
        want: Some(&Predicate::truthy_path("plugins.*.hostPath")),
    );
}

#[test]
fn stringified_trimmed_equality_uses_the_transformed_scalar_value() {
    let result = eval_expr(
        &single_expr(r#"eq (.Values.image.tag | toString | trimSuffix "-jmx") "7""#),
        &EvalEnv::default(),
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
        "the comparison must consume the post-transform scalar dispatch: {result:#?}"
    );
}

fn project_helper_arg(
    action: &str,
    outer: Option<&HashMap<String, AbstractValue>>,
) -> HashMap<String, AbstractValue> {
    let expr = single_expr(action);
    project_helper_arg_expr(&expr, outer)
}

fn project_helper_arg_expr(
    expr: &TemplateExpr,
    outer: Option<&HashMap<String, AbstractValue>>,
) -> HashMap<String, AbstractValue> {
    bindings_for_helper_arg_with(Some(expr), outer, |expr| match expr {
        TemplateExpr::Call { function, .. } if function == "fallback" => {
            EvalResult::from_value(AbstractValue::Dict(BTreeMap::from([(
                "fallback".to_string(),
                AbstractValue::ValuesPath("fallback.value".to_string()),
            )])))
        }
        TemplateExpr::Call { function, .. } if function == "overrideMap" => {
            EvalResult::from_value(AbstractValue::Dict(BTreeMap::from([(
                "fallback".to_string(),
                AbstractValue::ValuesPath("override".to_string()),
            )])))
        }
        _ => eval_expr(expr, &EvalEnv::default()),
    })
    .bindings
}

#[test]
fn helper_argument_dict_projects_string_and_raw_string_keys() {
    sim_assert_eq!(
        have: project_helper_arg(r#"dict "name" .Values.serviceAccount.name `raw` .Values.raw"#, None),
        want: HashMap::from([
            (
                "name".to_string(),
                AbstractValue::ValuesPath("serviceAccount.name".to_string()),
            ),
            (
                "raw".to_string(),
                AbstractValue::ValuesPath("raw".to_string()),
            ),
        ])
    );
}

#[test]
fn helper_argument_merge_preserves_ordered_overwrite_and_root_context_expansion() {
    let outer = HashMap::from([(
        "root".to_string(),
        AbstractValue::ValuesPath("root.value".to_string()),
    )]);
    let expr = TemplateExpr::Call {
        function: "merge".to_string(),
        args: vec![
            TemplateExpr::Call {
                function: "fallback".to_string(),
                args: Vec::new(),
            },
            TemplateExpr::Variable(String::new()),
            TemplateExpr::Call {
                function: "overrideMap".to_string(),
                args: Vec::new(),
            },
        ],
    };

    sim_assert_eq!(
        have: project_helper_arg_expr(&expr, Some(&outer)),
        want: HashMap::from([
            (
                "fallback".to_string(),
                AbstractValue::ValuesPath("override".to_string()),
            ),
            (
                "root".to_string(),
                AbstractValue::ValuesPath("root.value".to_string()),
            ),
        ])
    );
}

#[test]
fn json_roundtrip_preserves_input_identity_with_decoded_representation() {
    let result = eval_expr(
        &single_expr(".Values.extraResources | toJson | fromJson"),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::JsonDecodedPath("extraResources".to_string()))
    );
}

#[test]
fn json_roundtrip_preserves_values_root_inside_constructed_container() {
    let result = eval_expr(
        &single_expr(r#"get (dict "doc" .Values | toJson | fromJson) "doc""#),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::JsonDecodedPath(String::new())),
    );
}

#[test]
fn yaml_roundtrip_preserves_values_root_inside_constructed_container() {
    let result = eval_expr(
        &single_expr(r#"get (dict "doc" .Values | toYaml | fromYaml) "doc""#),
        &EvalEnv::default(),
    );

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::ValuesPath(String::new())),
    );
}

#[test]
fn from_json_without_matching_serialization_only_contracts_the_input_string() {
    let result = eval_expr(
        &single_expr(".Values.payload | fromJson"),
        &EvalEnv::default(),
    );

    sim_assert_eq!(have: result.value, want: None);
    sim_assert_eq!(
        have: result.effects.type_hints.get("payload"),
        want: Some(&["string".to_string()].into_iter().collect())
    );
}

#[test]
fn root_values_replacement_is_exported_and_used_by_later_values_reads() {
    let mut env = EvalEnv {
        dot: Some(AbstractValue::RootContext),
        ..EvalEnv::default()
    };
    let result = eval_expr(
        &single_expr(r#"set . "Values" (.Values | toJson | fromJson)"#),
        &env,
    );
    env.root_fields.extend(result.effects.root_set_mutations);

    sim_assert_eq!(
        have: eval_expr(&single_expr(".Values.extraResources"), &env).value,
        want: Some(AbstractValue::JsonDecodedPath("extraResources".to_string()))
    );
}

#[test]
fn root_set_truth_predicates_feed_later_root_field_assignments() {
    let mut env = EvalEnv {
        dot: Some(AbstractValue::RootContext),
        ..EvalEnv::default()
    };
    let server = eval_expr(
        &single_expr(indoc! {r#"
            set . "serverEnabled" (or
                            (eq (.Values.server.enabled | toString) "true")
                            (and
                                (eq (.Values.server.enabled | toString) "-")
                                (eq (.Values.global.enabled | toString) "true")))"#}),
        &env,
    );
    let enabled = Predicate::Or(vec![
        Predicate::Or(vec![
            Predicate::from(Guard::Eq {
                path: "server.enabled".to_string(),
                value: GuardValue::string("true"),
            }),
            Predicate::from(Guard::Eq {
                path: "server.enabled".to_string(),
                value: GuardValue::Bool(true),
            }),
        ]),
        Predicate::And(vec![
            Predicate::from(Guard::Eq {
                path: "server.enabled".to_string(),
                value: GuardValue::string("-"),
            }),
            Predicate::Or(vec![
                Predicate::from(Guard::Eq {
                    path: "global.enabled".to_string(),
                    value: GuardValue::string("true"),
                }),
                Predicate::from(Guard::Eq {
                    path: "global.enabled".to_string(),
                    value: GuardValue::Bool(true),
                }),
            ]),
        ]),
    ]);
    sim_assert_eq!(
        have: server.effects.root_set_predicates.get("serverEnabled"),
        want: Some(&enabled)
    );

    env.root_truthy_predicates
        .extend(server.effects.root_set_predicates);
    let server_enabled = eval_expr(&single_expr(".serverEnabled"), &env);
    sim_assert_eq!(
        have: server_enabled
            .output_reachability(SelectionPolarity::Truthy)
            .truth_source(),
        want: SelectionTruthSource::RenderedScalar
    );
    env.root_fields.insert(
        "mode".to_string(),
        AbstractValue::StringSet(BTreeSet::from(["server".to_string()])),
    );
    env.root_value_dispatches.insert(
        "mode".to_string(),
        ScalarValueDispatch::constant(GuardValue::string("server")),
    );
    let mode = eval_expr(&single_expr(".mode"), &env);
    sim_assert_eq!(
        have: mode
            .output_reachability(SelectionPolarity::Truthy)
            .truth_source(),
        want: SelectionTruthSource::RenderedScalar
    );
    let service = eval_expr(
        &single_expr(indoc! {r#"
            set . "serverServiceEnabled"
                            (and .serverEnabled
                                (eq (.Values.server.service.enabled | toString) "true"))"#}),
        &env,
    );
    sim_assert_eq!(
        have: service
            .effects
            .root_set_predicates
            .get("serverServiceEnabled"),
        want: Some(&Predicate::And(vec![
            enabled,
            Predicate::Or(vec![
                Predicate::from(Guard::Eq {
                    path: "server.service.enabled".to_string(),
                    value: GuardValue::string("true"),
                }),
                Predicate::from(Guard::Eq {
                    path: "server.service.enabled".to_string(),
                    value: GuardValue::Bool(true),
                }),
            ]),
        ]))
    );
}

#[test]
fn root_values_merge_records_the_fallback_values_subtree() {
    let env = EvalEnv {
        dot: Some(AbstractValue::RootContext),
        locals: HashMap::from([(
            "defaults".to_string(),
            AbstractValue::ValuesPath("_internal_defaults".to_string()),
        )]),
        ..EvalEnv::default()
    };
    let result = eval_expr(
        &single_expr(r#"set $ "Values" (mustMergeOverwrite $defaults $.Values)"#),
        &env,
    );

    sim_assert_eq!(
        have: result.effects.values_default_sources,
        want: BTreeSet::from([crate::ValuesDefaultSource {
            target_path: String::new(),
            source_path: "_internal_defaults".to_string(),
        }])
    );
}

#[test]
fn coercing_arithmetic_erases_raw_operand_shape() {
    // `mulf`/`divf`/`floor` cast operands through Sprig's numeric coercion,
    // so the raw operand's kind is unconstrained (Traefik's goMemLimit
    // arithmetic accepts numeric strings and junk that coerces to zero).
    let result = eval_expr(
        &single_expr(r"mulf .Values.pct 1048576.0 | divf 1048576.0 | floor"),
        &EvalEnv::default(),
    );
    assert!(
        result.effects.shape_erased_paths.contains("pct"),
        "arithmetic operand shape is coerced, not constrained: {:?}",
        result.effects.shape_erased_paths
    );
    sim_assert_eq!(
        have: result.effects.type_hints.get("pct"),
        want: None,
        "arithmetic must not type its raw operand"
    );
}

#[test]
fn division_operand_is_not_arithmetic_erased() {
    // Division/modulo keep a real zero-denominator precondition, so they are
    // deliberately excluded from the coercing-arithmetic widening.
    let result = eval_expr(&single_expr(r"div .Values.count 2"), &EvalEnv::default());
    assert!(
        !result.effects.shape_erased_paths.contains("count"),
        "div is not part of the coercing-arithmetic catalog"
    );
}

#[test]
fn finite_selector_program_construction_stays_exact() {
    let env = EvalEnv {
        locals: HashMap::from([(
            "dep".to_string(),
            AbstractValue::StringSet(BTreeSet::from([
                "telemetry.v2.stackdriver.disableOutbound".to_string()
            ])),
        )]),
        ..EvalEnv::default()
    };
    let result = eval_expr(
        &single_expr(
            r#"print "{{" (repeat (split "." $dep | len) "(") ".Values." (replace "." ")." $dep) ")}}""#,
        ),
        &env,
    );
    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::StringSet(BTreeSet::from([
            "{{((((.Values.telemetry).v2).stackdriver).disableOutbound)}}".to_string(),
        ])))
    );
}

/// A ternary's condition only SELECTS an arm: its identity must not join
/// the result's output paths, or the placement slot's provider schema
/// stamps onto the raw flag (harbor's `ternary "https-web" "http-web"
/// .Values.internalTLS.enabled` at a Service port-name slot). The Boolean
/// operand contract still rides the capture lane.
#[test]
fn ternary_condition_identity_stays_out_of_output_paths() {
    for action in [
        r#"ternary "https-web" "http-web" .Values.internalTLS.enabled"#,
        r#".Values.internalTLS.enabled | ternary "https-web" "http-web""#,
    ] {
        let result = eval_expr(&single_expr(action), &EvalEnv::default());

        assert!(
            !result.effects.output_paths.contains("internalTLS.enabled"),
            "the condition never renders into the slot: {action}"
        );
        assert!(
            result.effects.helper_fails.iter().any(|capture| matches!(
                &capture.kind,
                crate::eval_effect::CaptureKind::ComparableKind { path, schema_type }
                    | crate::eval_effect::CaptureKind::ValueType { path, schema_type, .. }
                    if path == "internalTLS.enabled" && schema_type == "boolean"
            )),
            "the Boolean operand contract must survive: {action}"
        );
    }
}

#[test]
fn ternary_condition_discards_local_output_metadata_but_keeps_consumption_contracts() {
    let metadata = HelperOutputMeta {
        input_identity: true,
        predicates: BTreeSet::from([BTreeSet::from([Predicate::truthy_path(
            "diagnosticMode.enabled",
        )])]),
        ..HelperOutputMeta::default()
    };
    let env = EvalEnv {
        locals: HashMap::from([(
            "flag".to_string(),
            AbstractValue::ValuesPath("diagnosticMode.enabled".to_string()),
        )]),
        local_output_meta: HashMap::from([(
            "flag".to_string(),
            BTreeMap::from([("diagnosticMode.enabled".to_string(), metadata)]),
        )]),
        ..EvalEnv::default()
    };
    let result = eval_expr(
        &single_expr(r#"$flag | ternary "enabled" "disabled""#),
        &env,
    );

    sim_assert_eq!(
        have: result.effects.local_output_meta,
        want: BTreeMap::new()
    );
    assert!(
        result.effects.helper_fails.iter().any(|capture| matches!(
            &capture.kind,
            crate::eval_effect::CaptureKind::ComparableKind { path, schema_type }
                | crate::eval_effect::CaptureKind::ValueType { path, schema_type, .. }
                if path == "diagnosticMode.enabled" && schema_type == "boolean"
        )),
        "the Boolean consumption contract must survive without returned metadata: {result:#?}"
    );

    let nested_consumer = eval_expr(
        &single_expr(r#"empty (b64enc .Values.payload) | ternary "empty" "present""#),
        &EvalEnv::default(),
    );
    assert!(
        nested_consumer.effects.encoded_paths.contains("payload")
            && nested_consumer
                .effects
                .string_contract_paths
                .contains("payload"),
        "evaluating the predicate still runs its strict nested consumer: {nested_consumer:#?}"
    );
}

/// `merge` keeps the FIRST occurrence of a key across its arguments, so
/// values-backed operands form ordered layers with the destination first;
/// `mergeOverwrite` keeps the LAST and reverses them. An operand without a
/// single values identity abstains to the unordered fold.
#[test]
fn merge_of_values_paths_forms_ordered_layers() {
    let merged = eval_expr(
        &single_expr(r"merge (.Values.preferred | default dict) (.Values.legacy | default dict)"),
        &EvalEnv::default(),
    );
    let Some(AbstractValue::MergedLayers(layers)) = merged.value else {
        panic!(
            "merge of two values paths forms layers, got {:?}",
            merged.value
        );
    };
    sim_assert_eq!(
        have: layers
            .iter()
            .map(|layer| layer.paths().into_iter().collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        want: vec![vec!["preferred".to_string()], vec!["legacy".to_string()]]
    );

    let overwritten = eval_expr(
        &single_expr(
            r"mergeOverwrite (.Values.legacy | default dict) (.Values.preferred | default dict)",
        ),
        &EvalEnv::default(),
    );
    let Some(AbstractValue::MergedLayers(layers)) = overwritten.value else {
        panic!(
            "mergeOverwrite of two values paths forms layers, got {:?}",
            overwritten.value
        );
    };
    sim_assert_eq!(
        have: layers
            .iter()
            .map(|layer| layer.paths().into_iter().collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        want: vec![vec!["preferred".to_string()], vec!["legacy".to_string()]]
    );

    let literal_operand = eval_expr(
        &single_expr(r#"merge (dict "a" 1) .Values.legacy"#),
        &EvalEnv::default(),
    );
    assert!(
        !matches!(literal_operand.value, Some(AbstractValue::MergedLayers(_))),
        "a literal dict operand abstains from layering, got {:?}",
        literal_operand.value
    );
}
