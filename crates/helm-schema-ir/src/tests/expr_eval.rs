use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{
    apply_local_set_mutations_expr, bindings_for_helper_arg_with, direct_values_path, eval_expr,
    eval_exprs_effects,
};
use helm_schema_ast::parse_expr_text;
use helm_schema_ast::render_printf_string_sets;
use helm_schema_ast::{TemplateExpr, parse_action_expressions};
use helm_schema_core::{Guard, GuardValue, Predicate};
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
        want: Some(AbstractValue::Choice(
            [
                AbstractValue::ValuesPath("serviceAccount.name".to_string()),
                AbstractValue::StringSet(["x".to_string()].into_iter().collect()),
            ]
            .into_iter()
            .collect(),
        )),
    );
}

#[test]
fn helper_argument_projection_uses_shared_expression_eval() {
    let env = EvalEnv::from_helper_context(None, None);
    let bindings = bindings_for_helper_arg_with(
        Some(&expr(r#"dict "ctx" $ "config" .Values.serviceAccount"#)),
        None,
        |expr| {
            eval_expr(expr, &env)
                .value
                .map(|value| value.to_context_value())
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
    let result = eval_expr(&single_expr(r#".Values.flag | quote"#), &EvalEnv::default());

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
    let expr = single_expr(r#"index .Values.sentinel.externalAccess.service.loadBalancerIP 0"#);
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
        capture.member_access
            && capture.conjunction
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
        &single_expr(r#"(.Values.resources.limits).memory"#),
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
            capture.member_access
                && capture.conjunction.iter().any(|predicate| {
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
        &single_expr(r#".Values.resources.limits.memory"#),
        &EvalEnv::default(),
    );

    assert!(result.effects.helper_fails.iter().any(|capture| {
        capture.member_access
            && capture.conjunction
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

    let result = eval_expr(&single_expr(r#"$config.name"#), &env);
    sim_assert_eq!(
        have: result.effects.output_paths,
        want: BTreeSet::from(["generatedName".to_string()])
    );
}

#[test]
fn selector_on_local_dict_records_only_selected_child_reads() {
    let expr = single_expr(r#"$config.annotations"#);
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
}

#[test]
fn base64_pipeline_preserves_source_path() {
    let expr = single_expr(r#".Values.auth.password | toString | b64enc"#);
    let result = eval_expr(&expr, &EvalEnv::default());

    sim_assert_eq!(
        have: result.value,
        want: Some(AbstractValue::ValuesPath("auth.password".to_string()))
    );
}

#[test]
fn uniq_pipeline_preserves_local_list_items() {
    let expr = single_expr(r#"$pullSecrets | uniq"#);
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
fn integer_and_float_comparisons_keep_distinct_runtime_kinds() {
    let conjunctions = |action: &str| {
        eval_expr(&single_expr(action), &EvalEnv::default())
            .effects
            .helper_fails
            .into_iter()
            .map(|capture| capture.conjunction)
            .collect::<BTreeSet<_>>()
    };

    sim_assert_eq!(
        have: conjunctions("eq .Values.input 1"),
        want: BTreeSet::from([vec![Predicate::from(Guard::TypeIs {
            path: "input".to_string(),
            schema_type: "integer".to_string(),
        })
        .negated()]])
    );
    sim_assert_eq!(
        have: conjunctions("eq .Values.input 1.5"),
        want: BTreeSet::from([vec![Predicate::from(Guard::TypeIs {
            path: "input".to_string(),
            schema_type: "number".to_string(),
        })
        .negated()]])
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
    let failure_conjunctions = consumed
        .effects
        .helper_fails
        .into_iter()
        .map(|capture| capture.conjunction.into_iter().collect::<BTreeSet<_>>())
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(
        have: failure_conjunctions,
        want: BTreeSet::from([
            BTreeSet::from([
                Predicate::truthy_path("primary"),
                Predicate::from(Guard::TypeIs {
                    path: "primary".to_string(),
                    schema_type: "string".to_string(),
                })
                .negated(),
            ]),
            BTreeSet::from([
                Predicate::truthy_path("primary").negated(),
                Predicate::truthy_path("fallback"),
                Predicate::from(Guard::TypeIs {
                    path: "fallback".to_string(),
                    schema_type: "string".to_string(),
                })
                .negated(),
            ]),
        ])
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
            Some(AbstractValue::Dict(BTreeMap::from([(
                "fallback".to_string(),
                AbstractValue::ValuesPath("fallback.value".to_string()),
            )])))
        }
        TemplateExpr::Call { function, .. } if function == "overrideMap" => {
            Some(AbstractValue::Dict(BTreeMap::from([(
                "fallback".to_string(),
                AbstractValue::ValuesPath("override".to_string()),
            )])))
        }
        _ => eval_expr(expr, &EvalEnv::default()).value,
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
        &single_expr(
            r#"set . "serverEnabled" (or
                (eq (.Values.server.enabled | toString) "true")
                (and
                    (eq (.Values.server.enabled | toString) "-")
                    (eq (.Values.global.enabled | toString) "true")))"#,
        ),
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
    let service = eval_expr(
        &single_expr(
            r#"set . "serverServiceEnabled"
                (and .serverEnabled
                    (eq (.Values.server.service.enabled | toString) "true"))"#,
        ),
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
