use crate::abstract_value::AbstractValue;

pub(crate) type HelperBinding = AbstractValue;

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn choice(bindings: Vec<HelperBinding>) -> Option<HelperBinding> {
    AbstractValue::choice(bindings)
}
