use std::collections::{BTreeSet, HashMap};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::{DefineIndex, parse_action_expressions};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::{Guard, GuardValue};

use helm_schema_ast::{Literal, TemplateExpr};
use helm_schema_core::Predicate;

macro_rules! values_path {
    ($path:expr $(,)?) => {
        format!(
            ".Values{}",
            if $path.is_empty() {
                String::new()
            } else {
                format!(".{}", $path)
            }
        )
    };
}

fn parse_condition(text: &str) -> Vec<Guard> {
    let wrapped = format!("{{{{ {text} }}}}");
    let Some(top) = parse_action_expressions(&wrapped).into_iter().next() else {
        return Vec::new();
    };
    condition_context(HashMap::new())
        .condition_predicate_expr(&top)
        .contract_guards()
        .unwrap_or_default()
}

fn parse_condition_with_template_bindings(
    text: &str,
    template_bindings: HashMap<String, String>,
) -> Vec<Guard> {
    parse_condition_with_template_facts(text, template_bindings)
}

#[test]
fn invalid_kind_is_absence_or_null_instead_of_truthiness() {
    let invalid = parse_action_expressions(r#"{{ kindIs "invalid" .Values.hostUsers }}"#)
        .into_iter()
        .next()
        .expect("condition expression");
    let predicate = condition_context(HashMap::new()).condition_predicate_expr(&invalid);
    sim_assert_eq!(
        have: predicate,
        want: Predicate::Or(vec![
            Predicate::from(Guard::Eq {
                path: helm_schema_core::ValuesPath::parse("hostUsers"),
                value: GuardValue::Null,
            }),
            Predicate::from(Guard::Absent {
                path: helm_schema_core::ValuesPath::parse("hostUsers"),
            }),
        ]),
    );

    let present = parse_action_expressions(r#"{{ not (kindIs "invalid" .Values.hostUsers) }}"#)
        .into_iter()
        .next()
        .expect("condition expression");
    let predicate = condition_context(HashMap::new()).condition_predicate_expr(&present);
    assert!(
        matches!(predicate.kind(), helm_schema_core::PredicateKind::Not(inner) if matches!(inner.kind(), helm_schema_core::PredicateKind::Or(_))),
        "negating invalid must preserve required-and-non-null semantics: {predicate:#?}"
    );
}

#[test]
fn invalid_kind_guard_tracks_nil_and_selected_input_arms() -> eyre::Result<()> {
    let mut context = condition_context(HashMap::new());
    context.source.push_str(
        r#"{{ $selected := index (dict) "missing" }}{{ if .Values.enabled }}{{ $selected = .Values.value }}{{ end }}"#,
    );
    let expression = parse_action_expressions(r#"{{ kindIs "invalid" $selected }}"#)
        .into_iter()
        .next()
        .ok_or_eyre("condition expression missing")?;
    let enabled = Predicate::truthy_path("enabled");
    let nil = Predicate::Or(vec![
        Predicate::from(Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("value"),
            value: GuardValue::Null,
        }),
        Predicate::from(Guard::Absent {
            path: helm_schema_core::ValuesPath::parse("value"),
        }),
    ]);
    sim_assert_eq!(
        have: context.condition_predicate_expr(&expression),
        want: Predicate::Or(vec![enabled.negated(), Predicate::all(vec![enabled, nil])]).normalize_boolean()
    );
    Ok(())
}

#[test]
fn selected_binding_type_test_keeps_its_decision_condition() {
    let use_b = Predicate::truthy_path("useB");
    let mut context = condition_context(HashMap::new());
    context.source.push_str(
        "{{ $selected := .Values.a }}{{ if .Values.useB }}{{ $selected = .Values.b }}{{ end }}",
    );
    let expression = parse_action_expressions(r#"{{ kindIs "string" $selected }}"#)
        .into_iter()
        .next()
        .expect("condition expression");
    let string_a = Predicate::from(Guard::TypeIs {
        path: helm_schema_core::ValuesPath::parse("a"),
        schema_type: "string".to_string(),
    });
    let string_b = Predicate::from(Guard::TypeIs {
        path: helm_schema_core::ValuesPath::parse("b"),
        schema_type: "string".to_string(),
    });
    let expected = helm_schema_core::PredicateMemo::new().normalize(Predicate::Or(vec![
        Predicate::all(vec![use_b.clone(), string_b]),
        Predicate::all(vec![use_b.negated(), string_a]),
    ]));

    sim_assert_eq!(
        have: context.condition_predicate_expr(&expression),
        want: expected,
    );
}

fn parse_condition_with_template_facts(
    text: &str,
    template_bindings: HashMap<String, String>,
) -> Vec<Guard> {
    let wrapped = format!("{{{{ {text} }}}}");
    let Some(top) = parse_action_expressions(&wrapped).into_iter().next() else {
        return Vec::new();
    };
    condition_context(template_bindings)
        .condition_predicate_expr(&top)
        .contract_guards()
        .unwrap_or_default()
}

struct ConditionContext {
    source: String,
    defines: DefineIndex,
}

impl ConditionContext {
    fn execution(
        &self,
        expression: &TemplateExpr,
    ) -> (crate::value::DocumentExecution, helm_schema_syntax::Span) {
        let mut source = self.source.clone();
        source.push_str("{{ if ");
        source.push_str(&expression_source(expression));
        source.push_str(" }}");
        let start = source.len();
        source.push_str("# selected{{ end }}");
        let db = IrAnalysisDb::new(&self.defines);
        (
            crate::value::tests::execute_document(&source, None, &db),
            helm_schema_syntax::Span::new(start + 2, start + 2 + "selected".len()),
        )
    }

    fn condition_predicate_expr(&self, expression: &TemplateExpr) -> Predicate {
        let (execution, span) = self.execution(expression);
        execution.observed_marker_condition(span)
    }

    fn condition_lowering_is_faithful(&self, expression: &TemplateExpr) -> bool {
        let (execution, span) = self.execution(expression);
        execution.observed_marker_complete(span)
            && !execution
                .observed_marker_condition(span)
                .contains_approximation()
    }

    fn condition_lowering_is_usable_for_control(&self, expression: &TemplateExpr) -> bool {
        let (execution, _) = self.execution(expression);
        execution.text().next().is_some()
    }
}

fn expression_source(expression: &TemplateExpr) -> String {
    match expression {
        TemplateExpr::Literal(Literal::String(text) | Literal::RawString(text)) => {
            format!("{text:?}")
        }
        TemplateExpr::Literal(Literal::UnresolvedString(source))
        | TemplateExpr::Unknown(source) => source.clone(),
        TemplateExpr::Literal(Literal::Int(value)) => value.to_string(),
        TemplateExpr::Literal(Literal::Float(value)) => format!("{value:?}"),
        TemplateExpr::Literal(Literal::Bool(value)) => value.to_string(),
        TemplateExpr::Literal(Literal::Nil) => "nil".to_string(),
        TemplateExpr::Field(path) => format!(".{}", path.join(".")),
        TemplateExpr::Variable(name) => format!("${name}"),
        TemplateExpr::Selector { operand, path } => {
            format!("({}).{}", expression_source(operand), path.join("."))
        }
        TemplateExpr::Call { function, args } => {
            let args = args
                .iter()
                .map(|argument| match argument {
                    TemplateExpr::Call { .. } | TemplateExpr::Pipeline(_) => {
                        format!("({})", expression_source(argument))
                    }
                    _ => expression_source(argument),
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("{function} {args}")
        }
        TemplateExpr::Pipeline(stages) => stages
            .iter()
            .map(expression_source)
            .collect::<Vec<_>>()
            .join(" | "),
        TemplateExpr::Parenthesized(inner) => format!("({})", expression_source(inner)),
        TemplateExpr::VariableDefinition { name, value } => {
            format!("{name} := {}", expression_source(value))
        }
        TemplateExpr::Assignment { name, value } => {
            format!("{name} = {}", expression_source(value))
        }
    }
}

fn condition_context(template_bindings: HashMap<String, String>) -> ConditionContext {
    condition_context_with_defines(template_bindings, DefineIndex::new())
}

fn condition_context_with_defines(
    template_bindings: HashMap<String, String>,
    defines: DefineIndex,
) -> ConditionContext {
    let mut source = String::new();
    for (name, value) in template_bindings {
        source.push_str("{{ $");
        source.push_str(&name);
        source.push_str(" := ");
        source.push_str(&value);
        source.push_str(" }}");
    }
    ConditionContext { source, defines }
}

#[test]
fn negated_include_uses_rendered_text_truthiness() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "<inline:rendered-false>",
        indoc::indoc! {r#"
            {{- define "rendered.false" -}}
            {{- if .Values.flag -}}
            false
            {{- else -}}
            false
            {{- end -}}
            {{- end -}}
        "#},
    );
    let expr = parse_action_expressions(r#"{{ not (include "rendered.false" .) }}"#)
        .into_iter()
        .next()
        .expect("condition expression");

    sim_assert_eq!(
        have: condition_context_with_defines(HashMap::new(), defines)
            .condition_predicate_expr(&expr),
        want: Predicate::False,
    );
}

#[test]
fn truthy_simple_path() {
    sim_assert_eq!(
        have: parse_condition(".Values.X"),
        want: vec![Guard::Truthy { path: helm_schema_core::ValuesPath::parse("X") }],
    );
}

#[test]
fn not_simple_path() {
    sim_assert_eq!(
        have: parse_condition("not .Values.X"),
        want: vec![Guard::Not { path: helm_schema_core::ValuesPath::parse("X") }],
    );
}

#[test]
fn absent_custom_root_field_is_false_until_set() {
    let expr = parse_action_expressions("{{ not .defaultValuesSet }}")
        .into_iter()
        .next()
        .expect("parsed root-field condition");
    let context = condition_context(HashMap::new());

    assert!(context.condition_lowering_is_faithful(&expr));
    sim_assert_eq!(
        have: context.condition_predicate_expr(&expr),
        want: Predicate::True,
    );
}

#[test]
fn composite_truthiness_is_faithful_only_when_its_exact_decoder_succeeds() {
    let undecodable_merge = "(merge .Values.base .Values.__unknown_fixture)".to_owned();
    let undecodable_selection = "(coalesce .Values.primary .Values.__unknown_fixture)".to_owned();
    let bindings = HashMap::from([
        (
            "context".to_string(),
            format!("(dict \"merged\" {undecodable_merge})"),
        ),
        ("selected".to_string(), undecodable_selection),
    ]);
    let context = condition_context(bindings);
    let merged = parse_action_expressions("{{ $context.merged }}")
        .into_iter()
        .next()
        .expect("merged selector condition");
    let selected = parse_action_expressions("{{ $selected }}")
        .into_iter()
        .next()
        .expect("first-truthy local condition");

    assert!(!context.condition_lowering_is_faithful(&merged));
    assert!(!context.condition_lowering_is_faithful(&selected));
    assert!(context.condition_lowering_is_usable_for_control(&merged));
    assert!(context.condition_lowering_is_usable_for_control(&selected));
}

#[test]
fn decodable_composite_truthiness_remains_faithful() {
    let bindings = HashMap::from([
        (
            "merged".to_string(),
            "(merge .Values.base .Values.override)".to_owned(),
        ),
        (
            "selected".to_string(),
            "(coalesce .Values.primary .Values.fallback)".to_owned(),
        ),
    ]);
    let context = condition_context(bindings);
    let merged = parse_action_expressions("{{ $merged }}")
        .into_iter()
        .next()
        .expect("merged local condition");
    let selected = parse_action_expressions("{{ $selected }}")
        .into_iter()
        .next()
        .expect("first-truthy local condition");

    assert!(context.condition_lowering_is_faithful(&merged));
    assert!(context.condition_lowering_is_faithful(&selected));
}

#[test]
fn quoted_empty_membership_preserves_false_and_zero_as_live_values() {
    let expr = parse_action_expressions(
        r#"{{ not (has (quote .Values.global.logLevel) (list "" (quote ""))) }}"#,
    )
    .into_iter()
    .next()
    .expect("condition expression");
    sim_assert_eq!(
        have: condition_context(HashMap::new()).condition_predicate_expr(&expr),
        want: Predicate::Not(Box::new(Predicate::Or(vec![
            Predicate::from(Guard::Absent {
                path: helm_schema_core::ValuesPath::parse("global.logLevel"),
            }),
            Predicate::from(Guard::Eq {
                path: helm_schema_core::ValuesPath::parse("global.logLevel"),
                value: GuardValue::Null,
            }),
            Predicate::from(Guard::Eq {
                path: helm_schema_core::ValuesPath::parse("global.logLevel"),
                value: GuardValue::string(""),
            }),
        ]))),
    );
}

#[test]
fn or_with_two_paths_emits_or_guard() {
    sim_assert_eq!(
        have: parse_condition("or .Values.A .Values.B"),
        want: vec![Guard::Or {
            paths: vec![
                helm_schema_core::ValuesPath::parse("A"),
                helm_schema_core::ValuesPath::parse("B"),
            ],
        }],
    );
}

#[test]
fn or_paths_are_sorted() {
    sim_assert_eq!(
        have: parse_condition("or .Values.z .Values.a"),
        want: vec![Guard::Or {
            paths: vec![
                helm_schema_core::ValuesPath::parse("a"),
                helm_schema_core::ValuesPath::parse("z"),
            ],
        }],
    );
}

#[test]
#[ignore = "R3b regression vs main, package C (parser, collection and validation transfers): has with a scalar haystack aborts before or can continue; tracked in plan/value-algebra-r3b.md follow-ups"]
fn has_with_a_scalar_haystack_aborts_before_or_can_continue() -> eyre::Result<()> {
    let expression = parse_action_expressions("{{ or (has .Values.A 1) (has .Values.B 2) }}")
        .into_iter()
        .next()
        .ok_or_eyre("condition expression missing")?;
    let context = condition_context(HashMap::new());
    let (execution, span) = context.execution(&expression);
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::True);
    sim_assert_eq!(have: execution.observed_marker_condition(span), want: Predicate::False);
    Ok(())
}

#[test]
fn or_with_equality_preserves_typed_alternative() {
    sim_assert_eq!(
        have: parse_condition(r#"or (eq .Values.mode "prod") .Values.enabled"#),
        want: vec![Guard::AnyOf {
            alternatives: vec![
                vec![Guard::Truthy {
                    path: helm_schema_core::ValuesPath::parse("enabled"),
                }],
                vec![Guard::Eq {
                    path: helm_schema_core::ValuesPath::parse("mode"),
                    value: GuardValue::string("prod"),
                }],
            ],
        }, comparison_normality("mode", "string")],
    );
}

/// The conjunctive alternative survives as a conjunction: the condition is compared by its
/// decisions, not its normal form, and must reproduce Helm's verdicts
/// (transfer/probes/t2-predicates.txt `or-and-*`).
#[test]
fn or_with_nested_and_preserves_conjunctive_alternative() -> eyre::Result<()> {
    use crate::tests::guard_oracle::{
        assert_equivalent, assert_helm_verdicts, conjunction, representatives,
    };
    let wrapped = r#"{{ or (and .Values.a .Values.b) (eq .Values.mode "prod") }}"#;
    let expression = parse_action_expressions(wrapped)
        .into_iter()
        .next()
        .ok_or_eyre("condition expression")?;
    let have = condition_context(HashMap::new()).condition_predicate_expr(&expression);
    let want = conjunction(&[Guard::AnyOf {
        alternatives: vec![
            vec![
                Guard::Truthy {
                    path: helm_schema_core::ValuesPath::parse("a"),
                },
                Guard::Truthy {
                    path: helm_schema_core::ValuesPath::parse("b"),
                },
            ],
            vec![Guard::Eq {
                path: helm_schema_core::ValuesPath::parse("mode"),
                value: GuardValue::string("prod"),
            }],
        ],
    }]);
    assert_equivalent(&have, &want, &representatives(&["prod"]))?;
    assert_helm_verdicts(
        &want,
        &[
            (serde_json::json!({"a": true, "b": 1}), true),
            (serde_json::json!({"a": true}), false),
            (serde_json::json!({"mode": "prod"}), true),
            (serde_json::json!({"a": true, "mode": "prod"}), true),
            (serde_json::json!({"a": true, "mode": "dev"}), false),
            (serde_json::json!({"a": true, "b": true, "mode": 1}), true),
        ],
    )?;
    Ok(())
}

#[test]
fn eq_with_string_literal() {
    sim_assert_eq!(
        have: parse_condition(r#"eq .Values.X "value""#),
        want: vec![Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("X"),
            value: GuardValue::string("value"),
        }, comparison_normality("X", "string")],
    );
}

#[test]
fn eq_with_string_literal_containing_phantom_path() {
    sim_assert_eq!(
        have: parse_condition(r#"eq .Values.X ".Values.fake""#),
        want: vec![Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("X"),
            value: GuardValue::string(".Values.fake"),
        }, comparison_normality("X", "string")],
    );
}

#[test]
fn eq_with_bool_literal_preserves_exact_comparison() {
    sim_assert_eq!(
        have: parse_condition("eq .Values.enabled false"),
        want: vec![Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("enabled"),
            value: GuardValue::Bool(false),
        }, comparison_normality("enabled", "boolean")],
    );
}

#[test]
fn eq_with_int_literal_preserves_exact_comparison() {
    sim_assert_eq!(
        have: parse_condition("eq .Values.replicas 3"),
        want: vec![Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("replicas"),
            value: GuardValue::Int(3),
        }, comparison_normality("replicas", "integer")],
    );
}

#[test]
fn eq_with_nil_literal_preserves_exact_comparison() {
    let expressions = parse_action_expressions("{{ eq .Values.image.tag nil }}");
    let predicate = expressions
        .first()
        .map(|expression| condition_context(HashMap::new()).condition_predicate_expr(expression));
    sim_assert_eq!(
        have: predicate,
        want: Some(Predicate::Or(vec![
            Predicate::from(Guard::Eq {
                path: helm_schema_core::ValuesPath::parse("image.tag"),
                value: GuardValue::Null,
            }),
            Predicate::from(Guard::Absent {
                path: helm_schema_core::ValuesPath::parse("image.tag"),
            }),
        ])),
    );
}

#[test]
fn eq_two_inputs_does_not_imply_that_either_input_is_truthy() {
    sim_assert_eq!(have: parse_condition("eq .Values.X .Values.Y"), want: Vec::<Guard>::new());
}

#[test]
fn ne_with_string_literal_emits_not_eq() {
    sim_assert_eq!(
        have: parse_condition(r#"ne .Values.X "value""#),
        want: vec![Guard::NotEq {
            path: helm_schema_core::ValuesPath::parse("X"),
            value: GuardValue::string("value"),
        }, comparison_normality("X", "string")],
    );
}

#[test]
fn not_eq_literal_projects_to_not_eq() {
    sim_assert_eq!(
        have: parse_condition(r#"not (eq .Values.mode "disabled")"#),
        want: vec![Guard::NotEq {
            path: helm_schema_core::ValuesPath::parse("mode"),
            value: GuardValue::string("disabled"),
        }, comparison_normality("mode", "string")],
    );
}

#[test]
fn not_ne_literal_projects_to_eq() {
    sim_assert_eq!(
        have: parse_condition(r#"not (ne .Values.mode "disabled")"#),
        want: vec![Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("mode"),
            value: GuardValue::string("disabled"),
        }, comparison_normality("mode", "string")],
    );
}

#[test]
fn and_falls_through_to_per_path_truthy() {
    sim_assert_eq!(
        have: parse_condition("and .Values.A .Values.B"),
        want: vec![
            Guard::Truthy { path: helm_schema_core::ValuesPath::parse("A") },
            Guard::Truthy { path: helm_schema_core::ValuesPath::parse("B") },
        ],
    );
}

#[test]
fn and_with_parens_falls_through_to_per_path_truthy() {
    sim_assert_eq!(
        have: parse_condition("and (.Values.A) (.Values.B)"),
        want: vec![
            Guard::Truthy { path: helm_schema_core::ValuesPath::parse("A") },
            Guard::Truthy { path: helm_schema_core::ValuesPath::parse("B") },
        ],
    );
}

#[test]
fn and_preserves_nested_not_guard() {
    sim_assert_eq!(
        have: parse_condition(
            "and .Values.prometheus.enabled (not .Values.prometheus.podmonitor.enabled)"
        ),
        want: vec![
            Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse("prometheus.enabled")
            },
            Guard::Not {
                path: helm_schema_core::ValuesPath::parse("prometheus.podmonitor.enabled")
            },
        ],
    );
}

/// The nested `or` stays a disjunction under the conjunction: the guards are compared by
/// their decisions, not their normal form, and must reproduce Helm's verdicts
/// (transfer/probes/t2-predicates.txt `and-or-*`).
#[test]
fn and_preserves_nested_or_guard() -> eyre::Result<()> {
    use crate::tests::guard_oracle::{
        assert_equivalent, assert_helm_verdicts, conjunction, representatives,
    };
    let have = parse_condition(
        "and .Values.ldap.enabled (or .Values.ldap.bind_password .Values.ldap.bindpw)",
    );
    eyre::ensure!(!have.is_empty(), "the condition must lower to guards");
    let want = conjunction(&[
        Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("ldap.enabled"),
        },
        Guard::Or {
            paths: vec![
                helm_schema_core::ValuesPath::parse("ldap.bind_password"),
                helm_schema_core::ValuesPath::parse("ldap.bindpw"),
            ],
        },
    ]);
    assert_equivalent(&conjunction(&have), &want, &representatives(&[]))?;
    assert_helm_verdicts(
        &want,
        &[
            (
                serde_json::json!({"ldap": {"enabled": true, "bind_password": "p"}}),
                true,
            ),
            (
                serde_json::json!({"ldap": {"enabled": true, "bindpw": "p"}}),
                true,
            ),
            (
                serde_json::json!({"ldap": {"enabled": true, "bind_password": "p", "bindpw": "q"}}),
                true,
            ),
            (
                serde_json::json!({"ldap": {"enabled": true, "bind_password": ""}}),
                false,
            ),
            (
                serde_json::json!({"ldap": {"enabled": false, "bind_password": "p"}}),
                false,
            ),
        ],
    )?;
    Ok(())
}

#[test]
fn empty_path_is_falsey_guard() {
    sim_assert_eq!(
        have: parse_condition("empty .Values.service.loadBalancerIP"),
        want: vec![Guard::Not {
            path: helm_schema_core::ValuesPath::parse("service.loadBalancerIP")
        }],
    );
}

#[test]
fn not_empty_path_is_truthy_guard() {
    sim_assert_eq!(
        have: parse_condition("not (empty .Values.service.loadBalancerIP)"),
        want: vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("service.loadBalancerIP")
        }],
    );
}

#[test]
fn not_or_paths_uses_demorgan_negated_guards() {
    sim_assert_eq!(
        have: parse_condition("not (or .Values.serviceMonitor.enabled .Values.podMonitor.enabled)"),
        want: vec![
            Guard::Not {
                path: helm_schema_core::ValuesPath::parse("podMonitor.enabled")
            },
            Guard::Not {
                path: helm_schema_core::ValuesPath::parse("serviceMonitor.enabled")
            },
        ],
    );
}

#[test]
fn empty_condition_returns_empty() {
    sim_assert_eq!(have: parse_condition(""), want: [] as [Guard; 0]);
    sim_assert_eq!(have: parse_condition("   "), want: [] as [Guard; 0]);
}

#[test]
fn condition_without_values_reference_returns_empty() {
    sim_assert_eq!(have: parse_condition(".Chart.Name"), want: [] as [Guard; 0]);
    sim_assert_eq!(have: parse_condition("not (empty $var)"), want: [] as [Guard; 0]);
}

#[test]
fn has_key_on_known_dict_is_structural_not_value_truthy() {
    let template_bindings = HashMap::from([(
        "arg".to_string(),
        r#"(dict "customLabels" .Values.commonLabels "context" .)"#.to_owned(),
    )]);

    sim_assert_eq!(
        have: parse_condition_with_template_bindings(
            r#"and (hasKey $arg "customLabels") (hasKey $arg "context")"#,
            template_bindings,
        ),
        want: Vec::<Guard>::new(),
    );
}

#[test]
fn eq_value_preserves_literal_dot_star_substring() {
    sim_assert_eq!(
        have: parse_condition(r#"eq .Values.X "match.*foo""#),
        want: vec![Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("X"),
            value: GuardValue::string("match.*foo"),
        }, comparison_normality("X", "string")],
    );
}

#[test]
fn eq_value_preserves_dot_values_substring_inside_string() {
    sim_assert_eq!(
        have: parse_condition(r#"eq .Values.X ".Values.fake""#),
        want: vec![Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("X"),
            value: GuardValue::string(".Values.fake"),
        }, comparison_normality("X", "string")],
    );
}

#[test]
fn alias_comparison_preserves_typed_predicates() {
    let aliases = HashMap::from([("mode".to_string(), values_path!("service.type"))]);

    sim_assert_eq!(
        have: parse_condition_with_template_bindings(r#"eq $mode "ClusterIP""#, aliases),
        want: vec![Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("service.type"),
            value: GuardValue::string("ClusterIP"),
        }, comparison_normality("service.type", "string")],
    );
}

#[test]
fn output_meta_comparison_preserves_typed_predicates() -> eyre::Result<()> {
    let template_bindings = HashMap::from([(
        "customUser".to_string(),
        "(coalesce .Values.auth.username .Values.global.postgresql.auth.username)".to_owned(),
    )]);

    // Only the first truthy candidate is compared; incompatible selected types abort.
    let selected_non_postgres = |path: &str| {
        Predicate::all(vec![
            Predicate::truthy_path(path),
            Predicate::from(Guard::TypeIs {
                path: helm_schema_core::ValuesPath::parse(path),
                schema_type: "string".to_owned(),
            }),
            Predicate::from(Guard::NotEq {
                path: helm_schema_core::ValuesPath::parse(path),
                value: GuardValue::string("postgres"),
            }),
        ])
    };
    let primary_empty = Predicate::truthy_path("auth.username").negated();
    let fallback_empty = Predicate::truthy_path("global.postgresql.auth.username").negated();
    let expected = Predicate::Or(vec![
        selected_non_postgres("auth.username"),
        Predicate::all(vec![
            primary_empty.clone(),
            selected_non_postgres("global.postgresql.auth.username"),
        ]),
        // An entirely empty selection returns nil, which is unequal to the string.
        Predicate::all(vec![primary_empty, fallback_empty]),
    ]);
    let expression = parse_action_expressions(r#"{{ ne $customUser "postgres" }}"#)
        .into_iter()
        .next()
        .ok_or_eyre("missing coalesced comparison")?;
    let actual = condition_context(template_bindings).condition_predicate_expr(&expression);
    assert!(!actual.contains_approximation());
    sim_assert_eq!(have: actual.value_paths(), want: BTreeSet::from([
        helm_schema_core::ValuesPath::parse("auth.username"),
        helm_schema_core::ValuesPath::parse("global.postgresql.auth.username"),
    ]));
    let representatives = [
        None,
        Some(serde_json::json!(null)),
        Some(serde_json::json!(false)),
        Some(serde_json::json!(0)),
        Some(serde_json::json!("")),
        Some(serde_json::json!(true)),
        Some(serde_json::json!(1)),
        Some(serde_json::json!("alice")),
        Some(serde_json::json!("postgres")),
        Some(serde_json::json!([])),
        Some(serde_json::json!([1])),
        Some(serde_json::json!({})),
        Some(serde_json::json!({"key": 1})),
    ];
    assert_coalesce_quotient_complete(&representatives)?;
    // The supported atoms distinguish only these quotient classes, so their product is exhaustive.
    for primary in &representatives {
        for fallback in &representatives {
            let document = coalesce_document(primary.as_ref(), fallback.as_ref());
            sim_assert_eq!(
                have: coalesce_predicate_holds(&actual, &document)?,
                want: coalesce_predicate_holds(&expected, &document)?,
                "coalesced document: {document}",
            );
        }
    }
    Ok(())
}

/// A binding defaulted from a literal (`$mode := .Values.x | default
/// "skipIfMissing"`) compares exactly: against the fallback literal the
/// falsy states also satisfy the equality, and against any other literal
/// the falsy states certainly satisfy the negation (external-secrets'
/// deleted `renderMode` selects the default arm, not the invalid-mode
/// `fail`).
#[test]
fn defaulted_binding_comparison_carries_the_fallback_arm() -> eyre::Result<()> {
    use crate::tests::guard_oracle::{assert_equivalent, assert_helm_verdicts, representatives};
    let path = "serviceMonitor.renderMode";
    let context = || {
        condition_context(HashMap::from([(
            "mode".to_string(),
            format!(r#"(default "skipIfMissing" .Values.{path})"#),
        )]))
    };
    let parse = |text: &str| -> eyre::Result<TemplateExpr> {
        let wrapped = format!("{{{{ {text} }}}}");
        parse_action_expressions(&wrapped)
            .into_iter()
            .next()
            .ok_or_eyre("condition expression")
    };
    // `default` replaces every falsy value with the literal; a truthy non-string reaches `eq`/`ne`,
    // which abort on incompatible kinds in Helm (t2-predicates.txt `mode-*-true`), so the
    // condition is defined only on falsy values and strings.
    let leaves = representatives(&["skipIfMissing", "alwaysRender"])
        .into_iter()
        .filter(|leaf| {
            !crate::tests::guard_oracle::truthy(leaf.as_ref())
                || leaf.as_ref().is_some_and(serde_json::Value::is_string)
        })
        .collect::<Vec<_>>();
    // Each condition is compared by its decisions, not its normal form, and the expected
    // predicate must reproduce Helm's verdicts (transfer/probes/t2-predicates.txt `mode-*`).
    let check = |text: &str, want: Predicate, verdicts: [bool; 6]| -> eyre::Result<()> {
        let have = context().condition_predicate_expr(&parse(text)?);
        assert_equivalent(&have, &want, &leaves)?;
        let documents = [
            serde_json::json!({"serviceMonitor": {}}),
            serde_json::json!({"serviceMonitor": {"renderMode": null}}),
            serde_json::json!({"serviceMonitor": {"renderMode": ""}}),
            serde_json::json!({"serviceMonitor": {"renderMode": "skipIfMissing"}}),
            serde_json::json!({"serviceMonitor": {"renderMode": "alwaysRender"}}),
            serde_json::json!({"serviceMonitor": {"renderMode": "other"}}),
        ];
        assert_helm_verdicts(
            &want,
            &documents.into_iter().zip(verdicts).collect::<Vec<_>>(),
        )
    };
    let truthy = Predicate::truthy_path(path.to_string());

    check(
        r#"eq $mode "skipIfMissing""#,
        Predicate::Or(vec![
            Predicate::And(vec![
                truthy.clone(),
                Predicate::from(Guard::Eq {
                    path: helm_schema_core::ValuesPath::parse(path),
                    value: GuardValue::string("skipIfMissing"),
                }),
            ]),
            truthy.negated(),
        ]),
        [true, true, true, true, false, false],
    )?;

    check(
        r#"eq $mode "alwaysRender""#,
        Predicate::And(vec![
            truthy.clone(),
            Predicate::from(Guard::Eq {
                path: helm_schema_core::ValuesPath::parse(path),
                value: GuardValue::string("alwaysRender"),
            }),
        ]),
        [false, false, false, false, true, false],
    )?;

    check(
        r#"ne $mode "alwaysRender""#,
        Predicate::Or(vec![
            Predicate::And(vec![
                truthy.clone(),
                Predicate::from(Guard::NotEq {
                    path: helm_schema_core::ValuesPath::parse(path),
                    value: GuardValue::string("alwaysRender"),
                }),
            ]),
            truthy.negated(),
        ]),
        [true, true, true, true, false, true],
    )?;

    check(
        r#"ne $mode "skipIfMissing""#,
        Predicate::And(vec![
            truthy.clone(),
            Predicate::from(Guard::NotEq {
                path: helm_schema_core::ValuesPath::parse(path),
                value: GuardValue::string("skipIfMissing"),
            }),
        ]),
        [false, false, false, false, true, true],
    )?;
    Ok(())
}

#[test]
fn alias_or_predicate_projects_to_path_disjunction() {
    let aliases = HashMap::from([(
        "annotations".to_string(),
        "(coalesce .Values.global.annotations .Values.service.annotations)".to_owned(),
    )]);

    sim_assert_eq!(
        have: parse_condition_with_template_bindings("or $annotations .Values.service.labels", aliases),
        want: vec![Guard::Or {
            paths: vec![
                helm_schema_core::ValuesPath::parse("global.annotations"),
                helm_schema_core::ValuesPath::parse("service.annotations"),
                helm_schema_core::ValuesPath::parse("service.labels"),
            ],
        }],
    );
}

#[test]
fn with_predicates_preserve_header_projection_semantics() {
    let predicate = Predicate::all(vec![
        Predicate::truthy_path("service.enabled"),
        Predicate::from(Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("service.type"),
            value: GuardValue::string("ClusterIP"),
        }),
        Predicate::Or(vec![
            Predicate::truthy_path("service.annotations"),
            Predicate::truthy_path("global.annotations"),
        ]),
        Predicate::truthy_path("service.disabled").negated(),
    ]);

    let with_predicate = Predicate::all(predicate.with_context_predicates());

    sim_assert_eq!(
        have: with_predicate.contract_guards(),
        want: Some(vec![
            Guard::With {
                path: helm_schema_core::ValuesPath::parse("service.enabled"),
            },
            Guard::With {
                path: helm_schema_core::ValuesPath::parse("service.type"),
            },
            Guard::Eq {
                path: helm_schema_core::ValuesPath::parse("service.type"),
                value: GuardValue::string("ClusterIP"),
            },
            Guard::With {
                path: helm_schema_core::ValuesPath::parse("service.annotations"),
            },
            Guard::With {
                path: helm_schema_core::ValuesPath::parse("global.annotations"),
            },
            Guard::Or {
                paths: vec![
                    helm_schema_core::ValuesPath::parse("global.annotations"),
                    helm_schema_core::ValuesPath::parse("service.annotations"),
                ],
            },
            Guard::With {
                path: helm_schema_core::ValuesPath::parse("service.disabled"),
            },
            Guard::Not {
                path: helm_schema_core::ValuesPath::parse("service.disabled"),
            },
        ])
    );
    sim_assert_eq!(
        have: Predicate::all(Predicate::False.with_context_predicates()),
        want: Predicate::False,
    );
}

/// `Files.Get (printf "files/profile-%s.yaml" X)` truthiness decodes
/// to the FINITE disjunction over the chart's matching indexed files —
/// never an opaque truthy fallback (istiod profile selection shape).
#[test]
fn files_get_printf_condition_decodes_to_finite_name_disjunction() {
    let mut defines = DefineIndex::new();
    defines.add_file_source("files/profile-demo.yaml", "a: 1\n");
    defines.add_file_source("files/profile-ambient.yaml", "b: 2\n");
    defines.add_file_source("templates/other.yaml", "kind: ConfigMap\n");
    let context = condition_context_with_defines(HashMap::new(), defines);

    let wrapped = r#"{{ .Files.Get (printf "files/profile-%s.yaml" .Values.profile) }}"#;
    let top = parse_action_expressions(wrapped)
        .into_iter()
        .next()
        .expect("parsed condition expression");
    let guards = context.condition_predicate_expr(&top).contract_guards();

    sim_assert_eq!(
        have: guards,
        want: Some(vec![Guard::AnyOf {
            alternatives: vec![
                vec![Guard::Eq {
                    path: helm_schema_core::ValuesPath::parse("profile"),
                    value: GuardValue::string("ambient"),
                }],
                vec![Guard::Eq {
                    path: helm_schema_core::ValuesPath::parse("profile"),
                    value: GuardValue::string("demo"),
                }],
            ],
        }]),
    );
}

/// `kindOf (default TAIL .Values.tag)` describes `tag` only where `tag` is
/// truthy and the tail everywhere else. The path-keyed source map cannot
/// spell a literal or a constructed tail, so the decode abstains instead of
/// claiming `TypeIs(tag)` for the states that select the tail (Helm renders
/// `tag: 0` with a string tail and aborts with a map tail; ground truth
/// `round8-f69x-evidence/probe/helm-typeof.log`).
///
/// RED on 7c6a2f6e: `selection_chain_identity_paths` silently truncated
/// the chain to `[tag]`, and the decode read `tag` as the terminal fallback.
#[test]
fn type_descriptor_over_a_truncated_selection_chain_abstains() -> eyre::Result<()> {
    for tail in [r#""lit""#, r#"(dict "k" "v")"#] {
        let text = format!(r#"{{{{ ne (kindOf (default {tail} .Values.tag)) "string" }}}}"#);
        let expr = parse_action_expressions(&text)
            .into_iter()
            .next()
            .ok_or_eyre("condition expression")?;
        let context = condition_context(HashMap::new());
        assert!(
            !context.condition_lowering_is_faithful(&expr),
            "tail {tail}: {:#?}",
            context.condition_predicate_expr(&expr),
        );
    }
    Ok(())
}

fn comparison_normality(path: &str, schema_type: &str) -> Guard {
    Guard::AnyOf {
        alternatives: vec![
            vec![Guard::Eq {
                path: helm_schema_core::ValuesPath::parse(path),
                value: GuardValue::Null,
            }],
            vec![Guard::Absent {
                path: helm_schema_core::ValuesPath::parse(path),
            }],
            vec![Guard::TypeIs {
                path: helm_schema_core::ValuesPath::parse(path),
                schema_type: schema_type.to_owned(),
            }],
        ],
    }
}

fn coalesce_document(
    primary: Option<&serde_json::Value>,
    fallback: Option<&serde_json::Value>,
) -> serde_json::Value {
    let username = |value: Option<&serde_json::Value>| {
        let mut mapping = serde_json::Map::new();
        if let Some(value) = value {
            mapping.insert("username".to_owned(), value.clone());
        }
        mapping
    };
    serde_json::json!({
        "auth": username(primary),
        "global": {"postgresql": {"auth": username(fallback)}},
    })
}

fn coalesce_value<'a>(
    document: &'a serde_json::Value,
    path: &helm_schema_core::ValuesPath,
) -> eyre::Result<Option<&'a serde_json::Value>> {
    let mut value = Some(document);
    for segment in path.segments() {
        let key = segment
            .literal()
            .ok_or_eyre("coalesce quotient cannot evaluate wildcards")?;
        value = value.and_then(|value| value.get(key));
    }
    Ok(value)
}

fn coalesce_truthy(value: Option<&serde_json::Value>) -> bool {
    match value {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(value)) => *value,
        Some(serde_json::Value::Number(value)) => value.as_f64().is_some_and(|value| value != 0.0),
        Some(serde_json::Value::String(value)) => !value.is_empty(),
        Some(serde_json::Value::Array(value)) => !value.is_empty(),
        Some(serde_json::Value::Object(value)) => !value.is_empty(),
    }
}

fn coalesce_guard_holds(guard: &Guard, document: &serde_json::Value) -> eyre::Result<bool> {
    match guard {
        Guard::Truthy { path } => Ok(coalesce_truthy(coalesce_value(document, path)?)),
        Guard::Eq { path, value } | Guard::NotEq { path, value } => {
            let actual = coalesce_value(document, path)?;
            let equal = match value {
                GuardValue::Null => actual.is_none_or(serde_json::Value::is_null),
                GuardValue::String(value) if value == "postgres" => {
                    actual.and_then(serde_json::Value::as_str) == Some("postgres")
                }
                GuardValue::String(_)
                | GuardValue::Bool(_)
                | GuardValue::Int(_)
                | GuardValue::Float(_) => eyre::bail!("unsupported coalesce literal: {value:?}"),
            };
            Ok(equal == matches!(guard, Guard::Eq { path: _, value: _ }))
        }
        Guard::TypeIs { path, schema_type } if schema_type == "string" => {
            Ok(coalesce_value(document, path)?.is_some_and(serde_json::Value::is_string))
        }
        Guard::Absent { path } => Ok(coalesce_value(document, path)?.is_none()),
        Guard::Not { path: _ }
        | Guard::MatchesPattern {
            path: _,
            pattern: _,
            templated: _,
        }
        | Guard::MatchesValue {
            path: _,
            pattern: _,
        }
        | Guard::CollectionPattern {
            path: _,
            domain: _,
            quantifier: _,
            pattern: _,
        }
        | Guard::NotMatchesPattern {
            path: _,
            pattern: _,
        }
        | Guard::RangeKeyPrefix { path: _, prefix: _ }
        | Guard::RangeKeyEquals { path: _, key: _ }
        | Guard::RangeKeyMatches {
            path: _,
            pattern: _,
        }
        | Guard::Or { paths: _ }
        | Guard::AnyOf { alternatives: _ }
        | Guard::Range { path: _ }
        | Guard::With { path: _ }
        | Guard::Default { path: _ }
        | Guard::TypeIs {
            path: _,
            schema_type: _,
        }
        | Guard::NotTypeIs {
            path: _,
            schema_type: _,
        }
        | Guard::IntGt { path: _, bound: _ }
        | Guard::IntLt { path: _, bound: _ }
        | Guard::AtMostOneMember { path: _ }
        | Guard::MinMembers { path: _, bound: _ }
        | Guard::HasKey { path: _, key: _ }
        | Guard::NotHasKey { path: _, key: _ }
        | Guard::ContainsEquals { path: _, value: _ }
        | Guard::ContainsMemberEquals {
            path: _,
            member: _,
            value: _,
        }
        | Guard::ContainsTruthyMember { path: _, member: _ } => {
            eyre::bail!("unsupported coalesce guard: {guard:?}")
        }
    }
}

fn coalesce_predicate_holds(
    predicate: &Predicate,
    document: &serde_json::Value,
) -> eyre::Result<bool> {
    match predicate.kind() {
        helm_schema_core::PredicateKind::True => Ok(true),
        helm_schema_core::PredicateKind::False => Ok(false),
        helm_schema_core::PredicateKind::Guard(guard) => coalesce_guard_holds(guard, document),
        helm_schema_core::PredicateKind::Not(inner) => {
            Ok(!coalesce_predicate_holds(inner, document)?)
        }
        helm_schema_core::PredicateKind::And(parts) => {
            let mut result = true;
            for part in parts {
                result &= coalesce_predicate_holds(part, document)?;
            }
            Ok(result)
        }
        helm_schema_core::PredicateKind::Or(parts) => {
            let mut result = false;
            for part in parts {
                result |= coalesce_predicate_holds(part, document)?;
            }
            Ok(result)
        }
        helm_schema_core::PredicateKind::Approximate {
            marker,
            paths: _,
            role: _,
            sound_subset: _,
        } => eyre::bail!("coalesce quotient cannot certify approximation: {marker}"),
    }
}

fn assert_coalesce_quotient_complete(
    representatives: &[Option<serde_json::Value>],
) -> eyre::Result<()> {
    let path = helm_schema_core::ValuesPath::parse("auth.username");
    let truthy = Guard::Truthy { path: path.clone() };
    let nil = Guard::Eq {
        path: path.clone(),
        value: GuardValue::Null,
    };
    let postgres = Guard::Eq {
        path: path.clone(),
        value: GuardValue::string("postgres"),
    };
    let string = Guard::TypeIs {
        path: path.clone(),
        schema_type: "string".to_owned(),
    };
    let absent = Guard::Absent { path };
    let mut signatures = BTreeSet::new();
    for representative in representatives {
        let document = coalesce_document(representative.as_ref(), None);
        let mut signature = BTreeSet::new();
        for guard in [&truthy, &nil, &postgres, &string, &absent] {
            if coalesce_guard_holds(guard, &document)? {
                signature.insert(guard.clone());
            }
        }
        signatures.insert(signature);
    }
    sim_assert_eq!(have: signatures, want: BTreeSet::from([
        BTreeSet::from([nil.clone(), absent]),
        BTreeSet::from([nil]),
        BTreeSet::new(),
        BTreeSet::from([string.clone()]),
        BTreeSet::from([truthy.clone()]),
        BTreeSet::from([truthy.clone(), string.clone()]),
        BTreeSet::from([truthy, string, postgres]),
    ]));
    Ok(())
}
