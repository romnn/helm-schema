//! Helm comparison failures retain their complete input domain through schema projection.

use color_eyre::eyre;
use helm_schema_ast::DefineIndex;
use helm_schema_core::{ConditionalGuard, ContractSchemaSignals, GuardValue, ValuesPath};
use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::SymbolicIrContext;
use helm_schema_k8s::Chain;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

#[derive(Clone, Copy)]
enum AcceptedInput {
    Any,
    Nil,
    Nullable(InputKind),
}

#[derive(Clone, Copy)]
enum InputKind {
    Boolean,
    Number,
    Integer,
    String,
    Object,
    Array,
}

impl InputKind {
    fn schema_type(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::String => "string",
            Self::Object => "object",
            Self::Array => "array",
        }
    }

    fn accepts(self, value: &Value) -> bool {
        match self {
            Self::Boolean => value.is_boolean(),
            Self::Number => value.is_number(),
            Self::Integer => value.as_f64().is_some_and(|value| value.fract() == 0.0),
            Self::String => value.is_string(),
            Self::Object => value.is_object(),
            Self::Array => value.is_array(),
        }
    }
}

struct Case {
    right: &'static str,
    accepted: AcceptedInput,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            right: "1",
            accepted: AcceptedInput::Nullable(InputKind::Integer),
        },
        Case {
            right: "1.5",
            accepted: AcceptedInput::Nullable(InputKind::Number),
        },
        Case {
            right: "true",
            accepted: AcceptedInput::Nullable(InputKind::Boolean),
        },
        Case {
            right: r#""x""#,
            accepted: AcceptedInput::Nullable(InputKind::String),
        },
        Case {
            right: "nil",
            accepted: AcceptedInput::Any,
        },
        Case {
            right: "(dict)",
            accepted: AcceptedInput::Nil,
        },
        Case {
            right: "(list)",
            accepted: AcceptedInput::Nil,
        },
        Case {
            right: r#"(fromJson "null")"#,
            accepted: AcceptedInput::Nullable(InputKind::Object),
        },
        Case {
            right: r#"(fromJsonArray "null")"#,
            accepted: AcceptedInput::Nullable(InputKind::Array),
        },
    ]
}

fn schema(signals: &ContractSchemaSignals) -> Value {
    generate_values_schema(ValuesSchemaInput::new(signals, &Chain::new(Vec::new())))
}

fn observed(operator: &str, case: &Case) -> Value {
    let context = SymbolicIrContext::new(&DefineIndex::new());
    let source = format!("{{{{ $_ := {operator} .Values.input {} }}}}", case.right);
    schema(
        &context
            .generate_contract_ir(&source)
            .finalize()
            .into_schema_signals(),
    )
}

fn expected(case: &Case) -> Value {
    let context = SymbolicIrContext::new(&DefineIndex::new());
    let references = context
        .generate_contract_ir("{{ $_ := .Values.input }}")
        .finalize()
        .into_schema_signals();
    let clauses = match case.accepted {
        AcceptedInput::Any => Vec::new(),
        AcceptedInput::Nil | AcceptedInput::Nullable(_) => {
            let path = ValuesPath::parse("input");
            let mut accepted = vec![
                ConditionalGuard::Eq {
                    path: path.clone(),
                    value: GuardValue::Null,
                },
                ConditionalGuard::Absent { path: path.clone() },
            ];
            if let AcceptedInput::Nullable(kind) = case.accepted {
                accepted.push(ConditionalGuard::TypeIs {
                    path,
                    schema_type: kind.schema_type().to_owned(),
                });
            }
            vec![vec![ConditionalGuard::Not(std::sync::Arc::new(
                ConditionalGuard::AnyOf(std::sync::Arc::new(accepted)),
            ))]]
        }
    };
    schema(&ContractSchemaSignals::new(
        references.schema_evidence_by_value_path().clone(),
        clauses,
    ))
}

#[test]
fn equality_type_failures_match_the_complete_expected_schema() {
    for operator in ["eq", "ne"] {
        for case in cases() {
            sim_assert_eq!(have: observed(operator, &case), want: expected(&case), "{operator} {}", case.right);
        }
    }
}

#[test]
fn equality_schemas_preserve_missing_null_and_matching_helm_kinds() -> eyre::Result<()> {
    for operator in ["eq", "ne"] {
        for case in cases() {
            let schema = observed(operator, &case);
            let validator = jsonschema::validator_for(&schema)?;
            assert!(
                validator.is_valid(&json!({})),
                "missing input: {operator} {}; schema={schema}",
                case.right
            );
            for value in [
                Value::Null,
                json!(false),
                json!(true),
                json!(0),
                json!(1),
                json!(1.5),
                json!(""),
                json!("x"),
                json!([]),
                json!([1]),
                json!({}),
                json!({"x": 1}),
            ] {
                let expected = match case.accepted {
                    AcceptedInput::Any => true,
                    AcceptedInput::Nil => value.is_null(),
                    AcceptedInput::Nullable(kind) => value.is_null() || kind.accepts(&value),
                };
                let instance = json!({"input": value});
                sim_assert_eq!(have: validator.is_valid(&instance), want: expected, "{operator} {}: {instance}; schema={schema}", case.right);
            }
        }
    }
    Ok(())
}
