use std::rc::Rc;

use helm_schema_core::{GuardValue, Predicate};

use super::{Arena, Coverage, Datum, Field, FrozenTest, Leaf, Reach, ScalarType, Value, ValueNode};
use crate::value::scalar_value::ScalarContent;

impl FrozenTest {
    pub(super) fn constant_outcome(&self) -> Option<bool> {
        if self.coverage != Coverage::Exact {
            return None;
        }
        if self.when_true == Predicate::True && self.when_false == Predicate::False {
            Some(true)
        } else if self.when_false == Predicate::True && self.when_true == Predicate::False {
            Some(false)
        } else {
            None
        }
    }
}

impl Value {
    pub(super) fn scalar(go_type: ScalarType, content: Option<ScalarContent>) -> Self {
        Self::leaf(Datum::Scalar { go_type, content })
    }

    pub(super) fn string(text: &str) -> Self {
        Self::scalar(
            ScalarType::String,
            Some(ScalarContent::Literal(GuardValue::String(text.to_owned()))),
        )
    }

    pub(super) fn leaf(datum: Datum) -> Self {
        Self {
            node: Rc::new(ValueNode::Leaf(Leaf {
                datum,
                origin: super::Origin::default(),
            })),
        }
    }

    pub(super) fn unknown() -> Self {
        Self::leaf(Datum::Unknown(super::UnknownValue {
            types: super::TypeCoverage::Unresolved,
            aliases: Reach::Top,
        }))
    }

    /// A repeated decision selects its matching arm, retaining correlation without a product.
    pub(super) fn select(decision: Rc<FrozenTest>, mut yes: Self, mut no: Self) -> Self {
        if let Some(taken) = decision.constant_outcome() {
            return if taken {
                yes = yes.under(&decision, true);
                yes
            } else {
                no = no.under(&decision, false);
                no
            };
        }
        yes = yes.under(&decision, true);
        no = no.under(&decision, false);
        if yes == no {
            return yes;
        }
        Self {
            node: Rc::new(ValueNode::Select { decision, yes, no }),
        }
    }

    pub(super) fn under(&self, selected: &FrozenTest, taken: bool) -> Self {
        self.restricted(&|decision| selected.implies_outcome(taken, decision))
    }

    pub(super) fn under_runtime(&self, guard: &super::guard::RuntimeGuard) -> Self {
        self.restricted(&|decision| guard.implies_test(decision))
    }

    fn restricted(&self, outcome: &dyn Fn(&FrozenTest) -> Option<bool>) -> Self {
        match self.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum: Datum::Text(text),
                origin: _,
            }) => text.projected(self, outcome),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Map { view: _, target: _ }
                    | Datum::Sequence(_)
                    | Datum::Strings(_)
                    | Datum::Slot {
                        backing: _,
                        index: _,
                    }
                    | Datum::Unknown(_),
                origin: _,
            }) => self.clone(),
            ValueNode::Select { decision, yes, no } => {
                if let Some(taken) = outcome(decision) {
                    if taken {
                        yes.restricted(outcome)
                    } else {
                        no.restricted(outcome)
                    }
                } else {
                    let selected_yes = yes.restricted(&|test| {
                        outcome(test).or_else(|| decision.implies_outcome(true, test))
                    });
                    let selected_no = no.restricted(&|test| {
                        outcome(test).or_else(|| decision.implies_outcome(false, test))
                    });
                    if Rc::ptr_eq(&yes.node, &selected_yes.node)
                        && Rc::ptr_eq(&no.node, &selected_no.node)
                    {
                        return self.clone();
                    }
                    if selected_yes == selected_no {
                        return selected_yes;
                    }
                    // Retain the enclosing decision while text queries specialize under its arms.
                    Self {
                        node: Rc::new(ValueNode::Select {
                            decision: Rc::clone(decision),
                            yes: selected_yes,
                            no: selected_no,
                        }),
                    }
                }
            }
        }
    }
}

impl Arena {
    pub(super) fn decision(
        &self,
        when_true: Predicate,
        when_false: Predicate,
        coverage: Coverage,
    ) -> Rc<FrozenTest> {
        let key = (coverage == Coverage::Exact).then(|| super::guard::ExactDecisionKey {
            when_true: when_true.clone(),
            when_false: when_false.clone(),
        });
        if let Some(key) = &key
            && let Some(test) = self
                .exact_decisions
                .borrow()
                .get(key)
                .and_then(std::rc::Weak::upgrade)
        {
            return test;
        }
        let id = self.next_decision.get();
        self.next_decision.set(id + 1);
        let test = Rc::new(FrozenTest {
            id,
            when_true,
            when_false,
            coverage,
            runtime: None,
        });
        if let Some(key) = key {
            self.exact_decisions
                .borrow_mut()
                .insert(key, Rc::downgrade(&test));
        }
        test
    }
}

impl Field {
    pub(super) fn select(decision: Rc<FrozenTest>, yes: Self, no: Self) -> Self {
        if let Some(taken) = decision.constant_outcome() {
            return if taken { yes } else { no };
        }
        if yes == no {
            return yes;
        }
        match (yes, no) {
            (Self::Present(yes), Self::Present(no)) => {
                Self::Present(Value::select(decision, yes, no))
            }
            (yes, no) => Self::Select {
                decision,
                yes: Rc::new(yes),
                no: Rc::new(no),
            },
        }
    }
}
