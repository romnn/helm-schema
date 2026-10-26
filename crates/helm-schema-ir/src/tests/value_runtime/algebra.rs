use std::collections::BTreeSet;
use std::rc::Rc;

use helm_schema_core::{GuardValue, Predicate};

use super::{
    AliasSet, Arena, Coverage, Datum, Field, FrozenTest, Leaf, ScalarType, Value, ValueNode,
};
use crate::value::scalar_value::ScalarContent;

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
                inputs: BTreeSet::new(),
            })),
        }
    }

    pub(super) fn unknown() -> Self {
        Self::leaf(Datum::Unknown(AliasSet::RenderGraph))
    }

    /// A repeated decision selects its matching arm, retaining correlation without a product.
    pub(super) fn select(decision: Rc<FrozenTest>, mut yes: Self, mut no: Self) -> Self {
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
        match self.node.as_ref() {
            ValueNode::Leaf(_) => self.clone(),
            ValueNode::Select { decision, yes, no } => {
                if decision.id == selected.id {
                    if taken {
                        yes.under(selected, taken)
                    } else {
                        no.under(selected, taken)
                    }
                } else {
                    Self::select(
                        Rc::clone(decision),
                        yes.under(selected, taken),
                        no.under(selected, taken),
                    )
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
        let id = self.decision.get();
        self.decision.set(id + 1);
        Rc::new(FrozenTest {
            id,
            when_true,
            when_false,
            coverage,
        })
    }
}

impl Field {
    pub(super) fn select(decision: Rc<FrozenTest>, yes: Self, no: Self) -> Self {
        if yes == no {
            return yes;
        }
        match (yes, no) {
            (Self::Present(yes), Self::Present(no)) => {
                Self::Present(Value::select(decision, yes, no))
            }
            (yes, no) => Self::Select {
                decision,
                yes: Box::new(yes),
                no: Box::new(no),
            },
        }
    }
}
