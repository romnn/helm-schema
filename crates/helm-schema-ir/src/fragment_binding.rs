use std::collections::BTreeSet;

use crate::abstract_value::AbstractValue;

pub(crate) type FragmentBinding = AbstractValue;

pub(crate) fn choice(bindings: Vec<FragmentBinding>) -> Option<FragmentBinding> {
    AbstractValue::choice(bindings)
}

pub(crate) fn union(
    left: Option<FragmentBinding>,
    right: Option<FragmentBinding>,
) -> Option<FragmentBinding> {
    match (left, right) {
        (Some(left), Some(right)) => choice(vec![left, right]),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

pub(crate) fn merge_all(bindings: Vec<FragmentBinding>) -> Option<FragmentBinding> {
    AbstractValue::merge_fragment_bindings(bindings)
}

pub(crate) fn remove_paths(
    binding: FragmentBinding,
    remove: &BTreeSet<String>,
) -> Option<FragmentBinding> {
    binding.remove_fragment_paths(remove)
}

#[cfg(test)]
pub(crate) fn values_root() -> FragmentBinding {
    AbstractValue::values_root()
}

#[cfg(test)]
pub(crate) fn output_set(paths: BTreeSet<String>) -> FragmentBinding {
    AbstractValue::fragment_output_paths(paths)
}
