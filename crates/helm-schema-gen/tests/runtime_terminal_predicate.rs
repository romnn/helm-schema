//! Preserve the complete executed abort formula through contract projection.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{
    ConditionalGuard, ContractRequirementImplication, ContractRequirementTarget,
    ContractSchemaSignals, FailValueRequirement, Guard, RequirementAtom, RequirementBdd,
    ValuesPath,
};
use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::SymbolicIrContext;
use helm_schema_k8s::Chain;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

#[derive(Clone, Copy)]
enum BooleanDomain {
    True,
    False,
    Either,
}

struct Case {
    name: &'static str,
    source: &'static str,
    domain: BooleanDomain,
    outer_gate: bool,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "selected empty string",
            source: r#"{{ $_ := required "bad" (ternary (dict) "" .Values.choose) }}"#,
            domain: BooleanDomain::True,
            outer_gate: false,
        },
        Case {
            name: "selected nil",
            source: r#"{{ $_ := required "bad" (ternary (dict) nil .Values.choose) }}"#,
            domain: BooleanDomain::True,
            outer_gate: false,
        },
        Case {
            name: "selected numeric condition",
            source: r#"{{ $_ := ternary "a" "b" (ternary false 0 .Values.choose) }}"#,
            domain: BooleanDomain::True,
            outer_gate: false,
        },
        Case {
            name: "reversed selected empty string",
            source: r#"{{ $_ := required "bad" (ternary "" (dict) .Values.choose) }}"#,
            domain: BooleanDomain::False,
            outer_gate: false,
        },
        Case {
            name: "selected booleans pass required",
            source: r#"{{ $_ := required "bad" (ternary false true .Values.choose) }}"#,
            domain: BooleanDomain::Either,
            outer_gate: false,
        },
        Case {
            name: "mixed outer gate and selected subject",
            source: r#"{{ if .Values.outer }}{{ $_ := required "bad" (ternary (dict) nil .Values.choose) }}{{ end }}"#,
            domain: BooleanDomain::True,
            outer_gate: true,
        },
    ]
}

fn schema_for_signals(signals: &ContractSchemaSignals) -> Value {
    generate_values_schema(ValuesSchemaInput::new(signals, &Chain::new(Vec::new())))
}

fn observed_schema(case: &Case) -> Value {
    let context = SymbolicIrContext::new(&DefineIndex::new());
    let signals = context
        .generate_contract_ir(case.source)
        .finalize()
        .into_schema_signals();
    schema_for_signals(&signals)
}

fn expected_schema(case: &Case) -> eyre::Result<Value> {
    let context = SymbolicIrContext::new(&DefineIndex::new());
    let reads = if case.outer_gate {
        "{{ $_ := .Values.choose }}{{ $_ := .Values.outer }}"
    } else {
        "{{ $_ := .Values.choose }}"
    };
    let references = context
        .generate_contract_ir(reads)
        .finalize()
        .into_schema_signals();
    let path = ValuesPath::parse("choose");
    let mut failing = vec![ConditionalGuard::Not(std::sync::Arc::new(
        ConditionalGuard::TypeIs {
            path: path.clone(),
            schema_type: "boolean".to_owned(),
        },
    ))];
    match case.domain {
        BooleanDomain::True => {
            failing.push(ConditionalGuard::Not(std::sync::Arc::new(
                ConditionalGuard::Truthy { path },
            )));
        }
        BooleanDomain::False => failing.push(ConditionalGuard::Truthy { path }),
        BooleanDomain::Either => {}
    }
    if case.outer_gate {
        if let Some(kind_failure) = failing.first_mut() {
            *kind_failure = ConditionalGuard::AllOf(
                vec![
                    ConditionalGuard::Truthy {
                        path: ValuesPath::parse("choose"),
                    },
                    kind_failure.clone(),
                ]
                .into(),
            );
        }
        failing = failing
            .into_iter()
            .map(|failure| {
                let mut terms = match failure {
                    ConditionalGuard::AllOf(terms) => std::sync::Arc::unwrap_or_clone(terms),
                    term => vec![term],
                };
                terms.push(ConditionalGuard::Truthy {
                    path: ValuesPath::parse("outer"),
                });
                ConditionalGuard::AllOf(terms.into())
            })
            .collect();
    }
    let mut clause = vec![match failing.as_slice() {
        [guard] => guard.clone(),
        _ => ConditionalGuard::AnyOf(failing.into()),
    }];
    ConditionalGuard::canonicalize_conjunction(&mut clause);
    // The expectation supplies the complete positive failure formula directly,
    // independently of the capture-to-requirement conversion under test.
    let mut evidence = references.schema_evidence_by_value_path().clone();
    let guards = if case.outer_gate {
        let atom = RequirementAtom::Value(Guard::Truthy {
            path: ValuesPath::parse("outer"),
        });
        let mut remaining = RequirementBdd::WORK_BUDGET;
        let mut bdd = RequirementBdd::new(std::slice::from_ref(&atom), &mut remaining)
            .ok_or_eyre("expected outer guard exceeds its budget")?;
        let root = bdd
            .atom(&atom)
            .ok_or_eyre("expected outer atom is missing")?;
        vec![
            bdd.finish(root)
                .ok_or_eyre("expected outer guard cannot export")?,
        ]
    } else {
        Vec::new()
    };
    evidence
        .entry(ValuesPath::parse("choose"))
        .or_default()
        .requirement_implications
        .push(ContractRequirementImplication::new(
            guards.clone(),
            ContractRequirementTarget::Value,
            vec![FailValueRequirement::SchemaTypeEvenNull(
                "boolean".to_owned(),
            )],
        ));
    let mut absent = guards;
    absent.push(ConditionalGuard::Absent {
        path: ValuesPath::parse("choose"),
    });
    ConditionalGuard::canonicalize_conjunction(&mut absent);
    let mut terminal = vec![clause, absent];
    terminal.sort();
    let expected = ContractSchemaSignals::new(evidence, terminal);
    Ok(schema_for_signals(&expected))
}

#[test]
fn selected_abort_projection_matches_the_complete_terminal_schema() -> eyre::Result<()> {
    for case in cases() {
        sim_assert_eq!(have: observed_schema(&case), want: expected_schema(&case)?, "{}", case.name);
    }
    Ok(())
}

#[test]
fn selected_abort_projection_preserves_the_full_input_type_domain() -> eyre::Result<()> {
    for case in cases() {
        let schema = observed_schema(&case);
        let validator = jsonschema::validator_for(&schema)?;
        for value in [
            Value::Null,
            json!(false),
            json!(true),
            json!(0),
            json!(1),
            json!(1.5),
            json!(""),
            json!("text"),
            json!([]),
            json!([1]),
            json!({}),
            json!({"x": 1}),
        ] {
            let want = match case.domain {
                BooleanDomain::True => value == json!(true),
                BooleanDomain::False => value == json!(false),
                BooleanDomain::Either => value.is_boolean(),
            };
            let instance = if case.outer_gate {
                json!({"outer": true, "choose": value.clone()})
            } else {
                json!({"choose": value.clone()})
            };
            sim_assert_eq!(have: validator.is_valid(&instance), want: want, "{}: {instance}; schema={schema}", case.name);
            if case.outer_gate {
                let dormant = json!({"outer": false, "choose": value});
                assert!(
                    validator.is_valid(&dormant),
                    "dormant consumer rejected {dormant}: {schema}"
                );
            }
        }
        let missing = if case.outer_gate {
            json!({"outer": true})
        } else {
            json!({})
        };
        assert!(
            !validator.is_valid(&missing),
            "missing reached condition must abort: {schema}"
        );
    }
    Ok(())
}
