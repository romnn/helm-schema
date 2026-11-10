//! Go template ordering comparisons and integer regions of numeric casts.
//!
//! `lt`/`le`/`gt`/`ge` compare two operands of one basic kind (integers of either sign compare
//! across). Other kinds abort the call. A comparison of a Sprig integer cast against a literal bound
//! lifts to its raw input as a certified pair of sound subsets: a raw JSON integer in the region
//! certainly satisfies it, and the generator adds the base-0 string spellings parsing into it.

use helm_schema_core::{Guard, GuardValue, Predicate};

use super::eval::normal;
use super::kinds::GoType;
use super::relationship::ValueOperation;
use super::{AbortReason, Coverage, Datum, Exit, Flow, Leaf, State, Value, ValueNode};

/// One integer comparison against a literal bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Order {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
    NotEqual,
}

impl Order {
    fn ordering(function: &str) -> Option<Self> {
        Some(match function {
            "lt" => Self::Less,
            "le" => Self::LessOrEqual,
            "gt" => Self::Greater,
            "ge" => Self::GreaterOrEqual,
            _ => return None,
        })
    }

    /// The same comparison with its operands swapped.
    fn mirrored(self) -> Self {
        match self {
            Self::Less => Self::Greater,
            Self::LessOrEqual => Self::GreaterOrEqual,
            Self::Greater => Self::Less,
            Self::GreaterOrEqual => Self::LessOrEqual,
            Self::Equal => Self::Equal,
            Self::NotEqual => Self::NotEqual,
        }
    }

    fn holds(self, value: i64, bound: i64) -> bool {
        match self {
            Self::Less => value < bound,
            Self::LessOrEqual => value <= bound,
            Self::Greater => value > bound,
            Self::GreaterOrEqual => value >= bound,
            Self::Equal => value == bound,
            Self::NotEqual => value != bound,
        }
    }

    /// The raw-integer subsets on which `int(value) OP bound` certainly holds and certainly fails.
    fn regions(self, path: &helm_schema_core::ValuesPath, bound: i64) -> Option<[Predicate; 2]> {
        let above = |bound: i64| {
            Predicate::from(Guard::IntGt {
                path: path.clone(),
                bound,
            })
        };
        let below = |bound: i64| {
            Predicate::from(Guard::IntLt {
                path: path.clone(),
                bound,
            })
        };
        let next = bound.checked_add(1)?;
        let previous = bound.checked_sub(1)?;
        let equal = Predicate::all(vec![above(previous), below(next)]);
        let different = Predicate::Or(vec![above(bound), below(bound)]);
        Some(match self {
            Self::Less => [below(bound), above(previous)],
            Self::LessOrEqual => [below(next), above(bound)],
            Self::Greater => [above(bound), below(next)],
            Self::GreaterOrEqual => [above(previous), below(bound)],
            Self::Equal => [equal, different],
            Self::NotEqual => [different, equal],
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BasicKind {
    Integer,
    Unsigned,
    Float,
    Text,
}

/// Go's `basicKind`: `Err` marks kinds every ordering comparison rejects.
fn basic_kind(kind: GoType) -> Option<Result<BasicKind, ()>> {
    Some(match kind {
        GoType::Int | GoType::Int64 => Ok(BasicKind::Integer),
        GoType::Uint64 => Ok(BasicKind::Unsigned),
        GoType::Float64 => Ok(BasicKind::Float),
        GoType::String => Ok(BasicKind::Text),
        GoType::Nil | GoType::Bool | GoType::Map | GoType::Slice => Err(()),
        GoType::Unknown => return None,
    })
}

/// Executes `lt`, `le`, `gt` or `ge`.
pub(super) fn ordering_invocation(state: State, function: &str, arguments: &[Value]) -> Vec<Exit> {
    let (Some(order), [left, right]) = (Order::ordering(function), arguments) else {
        let value = Value::influenced(super::operations::inputs(arguments, &state));
        return normal(state, value);
    };
    let left = state.consumed(left);
    let right = state.consumed(right);
    let kinds = [&left, &right].map(|value| basic_kind(state.heap.freeze(value).go_type()));
    let comparable = match kinds {
        [Some(Err(())), _] | [_, Some(Err(()))] => false,
        [Some(Ok(left)), Some(Ok(right))] => {
            left == right
                || matches!(
                    (left, right),
                    (BasicKind::Integer, BasicKind::Unsigned)
                        | (BasicKind::Unsigned, BasicKind::Integer)
                )
        }
        [Some(Ok(_)) | None, None] | [None, Some(Ok(_))] => true,
    };
    if !comparable {
        return vec![Exit {
            state,
            flow: Flow::Abort(AbortReason::Type),
        }];
    }
    let value = ordered_value(&state, order, &left, &right).unwrap_or_else(|| {
        Value::influenced(super::operations::inputs(
            &[left.clone(), right.clone()],
            &state,
        ))
    });
    normal(state, value)
}

fn ordered_value(state: &State, order: Order, left: &Value, right: &Value) -> Option<Value> {
    match (left.constant(), right.constant()) {
        (Some(GuardValue::Int(left)), Some(GuardValue::Int(right))) => {
            return Some(Value::literal(GuardValue::Bool(order.holds(*left, *right))));
        }
        (Some(GuardValue::String(left)), Some(GuardValue::String(right))) => {
            let holds = match order {
                Order::Less => left < right,
                Order::LessOrEqual => left <= right,
                Order::Greater => left > right,
                Order::GreaterOrEqual => left >= right,
                Order::Equal => left == right,
                Order::NotEqual => left != right,
            };
            return Some(Value::literal(GuardValue::Bool(holds)));
        }
        _ => {}
    }
    if let Some(GuardValue::Int(bound)) = right.constant() {
        return integer_comparison(state, left, order, *bound);
    }
    if let Some(GuardValue::Int(bound)) = left.constant() {
        return integer_comparison(state, right, order.mirrored(), *bound);
    }
    None
}

struct Region {
    holds: Vec<Predicate>,
    fails: Vec<Predicate>,
    exact: bool,
    remaining: usize,
}

/// Lifts `value OP bound` over an integer-valued operand to its inputs, or `None` when nothing
/// about the operand's inputs decides it.
pub(super) fn integer_comparison(
    state: &State,
    value: &Value,
    order: Order,
    bound: i64,
) -> Option<Value> {
    let mut region = Region {
        holds: Vec::new(),
        fails: Vec::new(),
        exact: true,
        remaining: super::MAX_SCALAR_DISPATCH_STATES,
    };
    integer_arm(&mut region, value, &Predicate::True, order, bound, false);
    let memo = &state.heap.arena.predicate_memo;
    let holds = memo.normalize(Predicate::Or(region.holds));
    let fails = memo.normalize(Predicate::Or(region.fails));
    if !region.exact && holds == Predicate::False && fails == Predicate::False {
        return None;
    }
    let influences = super::operations::inputs(std::slice::from_ref(value), state);
    Some(certified_truth(
        state,
        holds,
        fails,
        region.exact,
        influences,
    ))
}

/// A Boolean that is certainly true on `holds` and certainly false on `fails`.
///
/// Each certificate is its own exact decision, so control flow under it stays exact; the rest of
/// an inexact domain keeps an unknown Boolean over the operand's inputs.
pub(super) fn certified_truth(
    state: &State,
    holds: Predicate,
    fails: Predicate,
    exact: bool,
    influences: std::collections::BTreeSet<helm_schema_core::ValuesPath>,
) -> Value {
    let literal = |truth| Value::literal(GuardValue::Bool(truth));
    let select = |predicate: Predicate, yes: Value, no: Value| {
        if predicate == Predicate::False {
            return no;
        }
        let negated = predicate.clone().negated();
        Value::select(
            state
                .heap
                .arena
                .decision(predicate, negated, Coverage::Exact),
            yes,
            no,
        )
    };
    if exact {
        return select(holds, literal(true), literal(false));
    }
    let unknown = Value::scalar(super::ScalarType::Bool, None).with_influences(influences);
    select(holds, literal(true), select(fails, literal(false), unknown))
}

/// Walks one alternative; `cast` marks values read through a Sprig integer cast.
fn integer_arm(
    region: &mut Region,
    value: &Value,
    condition: &Predicate,
    order: Order,
    bound: i64,
    cast: bool,
) {
    if region.remaining == 0 {
        region.exact = false;
        return;
    }
    region.remaining -= 1;
    let (datum, origin) = match value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => {
            match decision.coverage {
                Coverage::Exact => {}
                Coverage::CertifiedPartial => region.exact = false,
                Coverage::Partial => {
                    region.exact = false;
                    return;
                }
            }
            if decision.when_true.contains_approximation()
                || decision.when_false.contains_approximation()
            {
                region.exact = false;
                return;
            }
            for (taken, arm) in [(&decision.when_true, yes), (&decision.when_false, no)] {
                let condition = Predicate::all(vec![condition.clone(), taken.clone()]);
                integer_arm(region, arm, &condition, order, bound, cast);
            }
            return;
        }
        ValueNode::Leaf(Leaf { datum, origin }) => (datum, origin),
    };
    let integer = match (value.constant(), cast) {
        (Some(GuardValue::Int(integer)), _) => Some(*integer),
        (Some(GuardValue::Null), true) => Some(0),
        (Some(GuardValue::Bool(truth)), true) => Some(i64::from(*truth)),
        _ => None,
    };
    if let Some(integer) = integer {
        let side = if order.holds(integer, bound) {
            &mut region.holds
        } else {
            &mut region.fails
        };
        side.push(condition.clone());
        return;
    }
    if cast
        && matches!(
            datum,
            Datum::Unknown(_)
                | Datum::Scalar {
                    go_type: _,
                    content: None
                }
        )
        && let Some(path) = origin.identity.as_ref()
        && let Some([holds, fails]) = order.regions(path, bound)
    {
        region.exact = false;
        region
            .holds
            .push(Predicate::all(vec![condition.clone(), holds]));
        region
            .fails
            .push(Predicate::all(vec![condition.clone(), fails]));
        return;
    }
    match origin.derivation.as_deref() {
        Some(derivation)
            if !cast
                && matches!(
                    derivation.operation,
                    ValueOperation::Numeric(super::ScalarType::Int | super::ScalarType::Int64)
                ) =>
        {
            integer_arm(
                region,
                &derivation.source.value,
                condition,
                order,
                bound,
                true,
            );
        }
        Some(_) | None => region.exact = false,
    }
}
