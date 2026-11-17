//! Correlated choices whose comparison and restriction work belongs to one object.

use std::rc::Rc;

use super::work::WorkBudget;
use super::{Field, FrozenTest, Leaf, Value, ValueNode};

pub(super) fn field(
    decision: &Rc<FrozenTest>,
    yes: Field,
    no: Field,
    budget: &mut WorkBudget,
) -> Option<Field> {
    if let Some(taken) = decision.constant_outcome() {
        return Some(if taken { yes } else { no });
    }
    if super::heap_equality::fields(&yes, &no, budget)? {
        return Some(yes);
    }
    Some(match (yes, no) {
        (Field::Present(yes), Field::Present(no)) => {
            Field::Present(value(decision, &yes, &no, budget)?)
        }
        (yes, no) => Field::Select {
            decision: Rc::clone(decision),
            yes: Rc::new(yes),
            no: Rc::new(no),
        },
    })
}

pub(super) fn value(
    decision: &Rc<FrozenTest>,
    yes: &Value,
    no: &Value,
    budget: &mut WorkBudget,
) -> Option<Value> {
    if let Some(taken) = decision.constant_outcome() {
        return restricted(
            if taken { yes } else { no },
            &|test| decision.implies_outcome(taken, test),
            budget,
        );
    }
    let yes = restricted(yes, &|test| decision.implies_outcome(true, test), budget)?;
    let no = restricted(no, &|test| decision.implies_outcome(false, test), budget)?;
    choice(decision, yes, no, budget)
}

/// Keeps already-specialized arms; restricting text again would recurse.
pub(super) fn choice(
    decision: &Rc<FrozenTest>,
    yes: Value,
    no: Value,
    budget: &mut WorkBudget,
) -> Option<Value> {
    if super::heap_equality::values(&yes, &no, budget)? {
        return Some(yes);
    }
    Some(Value {
        node: Rc::new(ValueNode::Select {
            decision: Rc::clone(decision),
            yes,
            no,
        }),
    })
}

fn restricted(
    value: &Value,
    outcome: &dyn Fn(&FrozenTest) -> Option<bool>,
    budget: &mut WorkBudget,
) -> Option<Value> {
    if !budget.charge(1) {
        return None;
    }
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: super::Datum::Text(text),
            origin: _,
        }) => text.projected_with_budget(value, outcome, budget),
        ValueNode::Leaf(_) => Some(value.clone()),
        ValueNode::Select { decision, yes, no } => {
            if let Some(taken) = outcome(decision) {
                return restricted(if taken { yes } else { no }, outcome, budget);
            }
            let selected_yes = restricted(
                yes,
                &|test| outcome(test).or_else(|| decision.implies_outcome(true, test)),
                budget,
            )?;
            let selected_no = restricted(
                no,
                &|test| outcome(test).or_else(|| decision.implies_outcome(false, test)),
                budget,
            )?;
            if Rc::ptr_eq(&yes.node, &selected_yes.node) && Rc::ptr_eq(&no.node, &selected_no.node)
            {
                return Some(value.clone());
            }
            choice(decision, selected_yes, selected_no, budget)
        }
    }
}
