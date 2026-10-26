use super::{
    ComparisonBudget, binary_comparison_acceptance, comparison_success, comparison_success_explored,
};
use crate::value::{Coverage, Datum, MapType, Remainder, State, Value};
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{GuardValue, Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

#[derive(Clone, Copy, Debug)]
enum Input {
    RawLeft,
    RawRight,
    Nil,
    Bool,
    String,
    Integer,
    Float,
    NilMap,
    Map,
    NilSlice,
    Slice,
    SelectedString,
    SelectedContainer,
    PartialString,
    PartialContainer,
}

impl Input {
    fn value(self, state: &mut State) -> Value {
        match self {
            Self::RawLeft => Value::input(ValuesPath::parse("left")),
            Self::RawRight => Value::input(ValuesPath::parse("right")),
            Self::Nil => Value::nil(),
            Self::Bool => Value::literal(GuardValue::Bool(true)),
            Self::String => Value::string("x"),
            Self::Integer => Value::literal(GuardValue::Int(1)),
            Self::Float => Value::literal(GuardValue::Float("1.5".to_owned())),
            Self::NilMap => Value::leaf(Datum::Map {
                view: MapType::Interface,
                target: None,
            }),
            Self::Map => state.heap.map(Remainder::Closed),
            Self::NilSlice => Value::nil_sequence(),
            Self::Slice => Value::sequence(Vec::new()),
            Self::SelectedString
            | Self::SelectedContainer
            | Self::PartialString
            | Self::PartialContainer => {
                let decision = if matches!(self, Self::PartialString | Self::PartialContainer) {
                    state
                        .heap
                        .arena
                        .decision(Predicate::False, Predicate::False, Coverage::Partial)
                } else {
                    let condition = Predicate::truthy_path("choose");
                    state.heap.arena.decision(
                        condition.clone(),
                        condition.negated(),
                        Coverage::Exact,
                    )
                };
                let (yes, no) = if matches!(self, Self::SelectedString | Self::PartialString) {
                    (Value::string("x"), Value::string("y"))
                } else {
                    (Self::NilMap.value(state), Self::Map.value(state))
                };
                Value::select(decision, yes, no)
            }
        }
    }
}

fn budget() -> ComparisonBudget {
    ComparisonBudget {
        remaining: crate::value::MAX_SCALAR_DISPATCH_STATES,
        truncated: false,
    }
}

#[test]
fn binary_comparison_domains_match_the_loader_explorer() {
    let inputs = [
        Input::RawLeft,
        Input::RawRight,
        Input::Nil,
        Input::Bool,
        Input::String,
        Input::Integer,
        Input::Float,
        Input::NilMap,
        Input::Map,
        Input::NilSlice,
        Input::Slice,
        Input::SelectedString,
        Input::SelectedContainer,
        Input::PartialString,
        Input::PartialContainer,
    ];
    for first in inputs {
        for second in inputs {
            for negated in [false, true] {
                let mut fast_state = State::new(Value::nil());
                let arguments = [first.value(&mut fast_state), second.value(&mut fast_state)];
                let have = comparison_success(
                    &mut fast_state,
                    &arguments.first().cloned().unwrap_or_else(Value::unknown),
                    &arguments,
                    negated,
                    &mut budget(),
                );
                let mut explored_state = State::new(Value::nil());
                let arguments = [
                    first.value(&mut explored_state),
                    second.value(&mut explored_state),
                ];
                let want = comparison_success_explored(
                    &mut explored_state,
                    &arguments.first().cloned().unwrap_or_else(Value::unknown),
                    &arguments,
                    negated,
                    &mut budget(),
                );
                sim_assert_eq!(have: have, want: want, "{first:?} / {second:?} / negated={negated}");
            }
        }
    }
}

#[test]
fn raw_binary_comparisons_do_not_enumerate_hypothetical_loaders() {
    for second in [
        Input::Nil,
        Input::Bool,
        Input::String,
        Input::Integer,
        Input::Float,
        Input::NilMap,
        Input::Map,
        Input::NilSlice,
        Input::Slice,
    ] {
        let mut state = State::new(Value::nil());
        let arguments = [Input::RawLeft.value(&mut state), second.value(&mut state)];
        let mut remaining = budget();
        let have = comparison_success(
            &mut state,
            &arguments.first().cloned().unwrap_or_else(Value::unknown),
            &arguments,
            false,
            &mut remaining,
        );
        sim_assert_eq!(have: remaining.remaining, want: crate::value::MAX_SCALAR_DISPATCH_STATES);
        sim_assert_eq!(have: remaining.truncated, want: false);
        sim_assert_eq!(have: have, want: binary_comparison_acceptance(&state, arguments.first().unwrap_or(&Value::nil()), &arguments));
        let mut explored = budget();
        let accepted = comparison_success_explored(
            &mut state,
            &arguments.first().cloned().unwrap_or_else(Value::unknown),
            &arguments,
            false,
            &mut explored,
        );
        assert!(accepted.is_some(), "{second:?}");
        assert!(explored.remaining < remaining.remaining, "{second:?}");
    }
}

#[test]
fn unresolved_pairs_variadic_and_selected_containers_keep_loader_dispatch() {
    let mut state = State::new(Value::nil());
    let unresolved = [
        Input::RawLeft.value(&mut state),
        Input::RawRight.value(&mut state),
    ];
    sim_assert_eq!(have: binary_comparison_acceptance(&state, unresolved.first().unwrap_or(&Value::nil()), &unresolved), want: None);
    let variadic = [
        Input::RawLeft.value(&mut state),
        Input::Integer.value(&mut state),
        Input::Float.value(&mut state),
    ];
    sim_assert_eq!(have: binary_comparison_acceptance(&state, variadic.first().unwrap_or(&Value::nil()), &variadic), want: None);
    let containers = [
        Input::SelectedContainer.value(&mut state),
        Input::Map.value(&mut state),
    ];
    sim_assert_eq!(have: binary_comparison_acceptance(&state, containers.first().unwrap_or(&Value::nil()), &containers), want: None);
    let partial_error = [
        Input::PartialString.value(&mut state),
        Input::Integer.value(&mut state),
    ];
    sim_assert_eq!(have: binary_comparison_acceptance(&state, partial_error.first().unwrap_or(&Value::nil()), &partial_error), want: None);
}

#[test]
fn contextual_numeric_literals_preserve_ieee_and_exact_integer_boundaries() -> eyre::Result<()> {
    use crate::value::kinds::GoType;
    use helm_schema_ast::{Literal, TemplateExpr};
    struct IntegerCase {
        input: i64,
        bits: u64,
    }
    struct FloatCase {
        input: f64,
        integer: i64,
    }
    for case in [
        IntegerCase {
            input: i64::MIN,
            bits: 0xc3e0_0000_0000_0000,
        },
        IntegerCase {
            input: i64::MAX,
            bits: 0x43e0_0000_0000_0000,
        },
        IntegerCase {
            input: 9_007_199_254_740_993,
            bits: 0x4340_0000_0000_0000,
        },
    ] {
        let value = super::contextual_literal(
            &TemplateExpr::Literal(Literal::Int(case.input)),
            Some(GoType::Float64),
        )
        .map_err(|reason| eyre::eyre!("{reason:?}"))?
        .ok_or_eyre("missing converted float")?;
        let Some(GuardValue::Float(number)) = value.constant() else {
            eyre::bail!("converted integer lost its float literal");
        };
        sim_assert_eq!(have: number.parse::<f64>()?.to_bits(), want: case.bits);
    }
    for case in [
        FloatCase {
            input: -0.0,
            integer: 0,
        },
        FloatCase {
            input: -9_007_199_254_740_991.0,
            integer: -9_007_199_254_740_991,
        },
        FloatCase {
            input: 9_007_199_254_740_991.0,
            integer: 9_007_199_254_740_991,
        },
    ] {
        let value = super::contextual_literal(
            &TemplateExpr::Literal(Literal::Float(case.input)),
            Some(GoType::Int64),
        )
        .map_err(|reason| eyre::eyre!("{reason:?}"))?
        .ok_or_eyre("missing converted integer")?;
        sim_assert_eq!(have: value.constant(), want: Some(&GuardValue::Int(case.integer)));
    }
    let beyond = super::contextual_literal(
        &TemplateExpr::Literal(Literal::Float(9_007_199_254_740_992.0)),
        Some(GoType::Int64),
    )
    .map_err(|reason| eyre::eyre!("{reason:?}"))?
    .ok_or_eyre("missing bounded conversion result")?;
    sim_assert_eq!(have: beyond.constant(), want: None);
    sim_assert_eq!(have: super::contextual_literal(&TemplateExpr::Literal(Literal::Float(0.5)), Some(GoType::Int64)), want: Err(crate::value::AbortReason::Type));
    Ok(())
}

#[test]
fn float_equality_keeps_go_signed_zero_and_nan_behavior() {
    let state = State::new(Value::nil());
    let zero = Value::literal(GuardValue::Float("0".to_owned()));
    let negative_zero = Value::literal(GuardValue::Float("-0".to_owned()));
    let nan = Value::literal(GuardValue::Float("NaN".to_owned()));
    sim_assert_eq!(have: super::scalar_equality(&state, &zero, &negative_zero, false).constant(), want: Some(&GuardValue::Bool(true)));
    sim_assert_eq!(have: super::scalar_equality(&state, &nan, &nan, false).constant(), want: Some(&GuardValue::Bool(false)));
}
