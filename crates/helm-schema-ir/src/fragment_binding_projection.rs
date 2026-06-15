use std::collections::BTreeSet;

use crate::abstract_value::AbstractValue;
use crate::fragment_binding::FragmentBinding;
use crate::helper_binding::HelperBinding;

pub(crate) fn fragment_to_current_dot_helper_binding(
    binding: &FragmentBinding,
) -> Option<HelperBinding> {
    // Current-dot path resolution only accepts bindings that structurally name
    // the root context or a `.Values` path. Rendered fragments are not a stable
    // substitute for the caller's lexical dot.
    match binding {
        FragmentBinding::ValuesPath(path) => Some(HelperBinding::ValuesPath(path.clone())),
        FragmentBinding::ValuesRoot => Some(HelperBinding::ValuesPath(String::new())),
        FragmentBinding::RootContext => Some(HelperBinding::RootContext),
        FragmentBinding::Unknown
        | FragmentBinding::Dict(_)
        | FragmentBinding::List(_)
        | FragmentBinding::Overlay { .. }
        | FragmentBinding::StringSet(_)
        | FragmentBinding::PathSet(_)
        | FragmentBinding::OutputSet(_)
        | FragmentBinding::Choice(_) => None,
    }
}

pub(crate) fn fragment_to_helper_binding(binding: &FragmentBinding) -> Option<HelperBinding> {
    AbstractValue::from_fragment_binding(binding).to_helper_binding()
}

pub(crate) fn fragment_source_paths(binding: &FragmentBinding) -> BTreeSet<String> {
    AbstractValue::from_fragment_output_binding(binding).shallow_paths()
}

pub(crate) fn fragment_rendered_paths(binding: &FragmentBinding) -> BTreeSet<String> {
    AbstractValue::from_fragment_output_binding(binding).paths()
}

pub(crate) fn fragment_strings(binding: &FragmentBinding) -> BTreeSet<String> {
    AbstractValue::from_fragment_binding(binding).strings()
}

pub(crate) fn select_fragment_binding(
    binding: &FragmentBinding,
    path: &[String],
) -> Option<FragmentBinding> {
    AbstractValue::from_fragment_binding(binding)
        .apply_to_path(path)
        .and_then(|value| value.to_fragment_binding())
}

pub(crate) fn fragment_item_binding(binding: &FragmentBinding) -> Option<FragmentBinding> {
    AbstractValue::from_fragment_binding(binding)
        .fragment_range_item()
        .and_then(|value| value.to_fragment_binding())
}

pub(crate) fn fragment_definitely_nonempty_iterable(binding: &FragmentBinding) -> bool {
    AbstractValue::from_fragment_binding(binding).definitely_nonempty_iterable()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{fragment_rendered_paths, fragment_source_paths};
    use crate::fragment_binding::FragmentBinding;

    #[test]
    fn values_root_abstains_from_fragment_path_extraction() {
        assert_eq!(
            fragment_source_paths(&FragmentBinding::ValuesRoot),
            BTreeSet::new()
        );
        assert_eq!(
            fragment_rendered_paths(&FragmentBinding::ValuesRoot),
            BTreeSet::new()
        );
    }

    #[test]
    fn fragment_paths_stay_shallow_while_rendered_paths_descend_structures() {
        let binding = FragmentBinding::Dict(BTreeMap::from([(
            "metadata".to_string(),
            FragmentBinding::ValuesPath("podLabels".to_string()),
        )]));

        assert_eq!(fragment_source_paths(&binding), BTreeSet::new());
        assert_eq!(
            fragment_rendered_paths(&binding),
            BTreeSet::from(["podLabels".to_string()])
        );
    }
}
