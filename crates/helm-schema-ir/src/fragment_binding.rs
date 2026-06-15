use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::helper_binding::HelperBinding;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FragmentBinding {
    ValuesPath(String),
    ValuesRoot,
    RootContext,
    Unknown,
    Dict(BTreeMap<String, FragmentBinding>),
    List(Vec<FragmentBinding>),
    Overlay {
        entries: BTreeMap<String, FragmentBinding>,
        fallback: Box<FragmentBinding>,
    },
    StringSet(BTreeSet<String>),
    PathSet(BTreeSet<String>),
    OutputSet(BTreeSet<String>),
    Choice(BTreeSet<FragmentBinding>),
}

impl FragmentBinding {
    pub(crate) fn to_current_dot_helper_binding(&self) -> Option<HelperBinding> {
        match self {
            Self::ValuesPath(path) => Some(HelperBinding::ValuesPath(path.clone())),
            Self::ValuesRoot => Some(HelperBinding::ValuesPath(String::new())),
            Self::RootContext => Some(HelperBinding::RootContext),
            Self::Unknown
            | Self::Dict(_)
            | Self::List(_)
            | Self::Overlay { .. }
            | Self::StringSet(_)
            | Self::PathSet(_)
            | Self::OutputSet(_)
            | Self::Choice(_) => None,
        }
    }

    pub(crate) fn to_helper_binding(&self) -> Option<HelperBinding> {
        AbstractValue::from_fragment_binding(self).to_helper_binding()
    }

    pub(crate) fn choice(bindings: Vec<Self>) -> Option<Self> {
        let mut flat = BTreeSet::new();
        for binding in bindings {
            match binding {
                Self::Choice(inner) => flat.extend(inner),
                other => {
                    flat.insert(other);
                }
            }
        }
        match flat.len() {
            0 => None,
            1 => flat.into_iter().next(),
            _ => Some(Self::Choice(flat)),
        }
    }

    pub(crate) fn paths(&self) -> BTreeSet<String> {
        AbstractValue::from_fragment_output_binding(self).shallow_paths()
    }

    pub(crate) fn rendered_paths(&self) -> BTreeSet<String> {
        AbstractValue::from_fragment_output_binding(self).paths()
    }

    pub(crate) fn strings(&self) -> BTreeSet<String> {
        AbstractValue::from_fragment_binding(self).strings()
    }

    pub(crate) fn union(left: Option<Self>, right: Option<Self>) -> Option<Self> {
        match (left, right) {
            (Some(left), Some(right)) => Self::choice(vec![left, right]),
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (None, None) => None,
        }
    }

    pub(crate) fn merge_all(bindings: Vec<Self>) -> Option<Self> {
        let mut map = BTreeMap::new();
        let mut non_dict_value_paths = BTreeSet::new();
        let mut non_dict_output_paths = BTreeSet::new();
        let mut non_dict_strings = BTreeSet::new();

        let mut pending = bindings;
        while let Some(binding) = pending.pop() {
            match binding {
                Self::Choice(choices) => {
                    pending.extend(choices);
                }
                Self::Dict(entries) => {
                    for (key, value) in entries {
                        let merged = Self::union(map.remove(&key), Some(value));
                        if let Some(merged) = merged {
                            map.insert(key, merged);
                        }
                    }
                }
                Self::Overlay { entries, fallback } => {
                    pending.push(*fallback);
                    for (key, value) in entries {
                        let merged = Self::union(map.remove(&key), Some(value));
                        if let Some(merged) = merged {
                            map.insert(key, merged);
                        }
                    }
                }
                Self::ValuesPath(path) => {
                    non_dict_value_paths.insert(path);
                }
                Self::ValuesRoot => {
                    non_dict_value_paths.insert(String::new());
                }
                Self::PathSet(paths) => {
                    non_dict_value_paths.extend(paths);
                }
                Self::OutputSet(paths) => {
                    non_dict_output_paths.extend(paths);
                }
                Self::StringSet(strings) => {
                    non_dict_strings.extend(strings);
                }
                Self::RootContext | Self::Unknown | Self::List(_) => {}
            }
        }

        let mut fallback_choices = Vec::new();
        if !non_dict_value_paths.is_empty() {
            fallback_choices.push(Self::PathSet(non_dict_value_paths));
        }
        if !non_dict_output_paths.is_empty() {
            fallback_choices.push(Self::OutputSet(non_dict_output_paths));
        }
        if !non_dict_strings.is_empty() {
            fallback_choices.push(Self::StringSet(non_dict_strings));
        }
        let fallback = Self::choice(fallback_choices);

        if map.is_empty() {
            fallback
        } else if let Some(fallback) = fallback {
            Some(Self::Overlay {
                entries: map,
                fallback: Box::new(fallback),
            })
        } else {
            Some(Self::Dict(map))
        }
    }

    pub(crate) fn remove_paths(self, remove: &BTreeSet<String>) -> Option<Self> {
        if remove.is_empty() {
            return Some(self);
        }

        match self {
            Self::ValuesPath(path) if remove.contains(&path) => None,
            Self::ValuesPath(_)
            | Self::ValuesRoot
            | Self::RootContext
            | Self::Unknown
            | Self::StringSet(_) => Some(self),
            Self::OutputSet(mut paths) => {
                paths.retain(|path| !remove.contains(path));
                if paths.is_empty() {
                    None
                } else {
                    Some(Self::OutputSet(paths))
                }
            }
            Self::PathSet(mut paths) => {
                paths.retain(|path| !remove.contains(path));
                if paths.is_empty() {
                    None
                } else {
                    Some(Self::PathSet(paths))
                }
            }
            Self::Dict(entries) => {
                let entries = entries
                    .into_iter()
                    .filter_map(|(key, value)| value.remove_paths(remove).map(|value| (key, value)))
                    .collect::<BTreeMap<_, _>>();
                if entries.is_empty() {
                    None
                } else {
                    Some(Self::Dict(entries))
                }
            }
            Self::List(items) => {
                let items = items
                    .into_iter()
                    .filter_map(|item| item.remove_paths(remove))
                    .collect::<Vec<_>>();
                if items.is_empty() {
                    None
                } else {
                    Some(Self::List(items))
                }
            }
            Self::Overlay { entries, fallback } => {
                let entries = entries
                    .into_iter()
                    .filter_map(|(key, value)| value.remove_paths(remove).map(|value| (key, value)))
                    .collect::<BTreeMap<_, _>>();
                match (entries.is_empty(), fallback.remove_paths(remove)) {
                    (true, fallback) => fallback,
                    (false, Some(fallback)) => Some(Self::Overlay {
                        entries,
                        fallback: Box::new(fallback),
                    }),
                    (false, None) => Some(Self::Dict(entries)),
                }
            }
            Self::Choice(choices) => Self::choice(
                choices
                    .into_iter()
                    .filter_map(|choice| choice.remove_paths(remove))
                    .collect(),
            ),
        }
    }

    pub(crate) fn apply_to_binding(&self, rest: &[String]) -> Option<Self> {
        AbstractValue::from_fragment_binding(self)
            .apply_to_path(rest)
            .and_then(|value| value.to_fragment_binding())
    }

    pub(crate) fn item_binding(&self) -> Option<Self> {
        AbstractValue::from_fragment_binding(self)
            .fragment_range_item()
            .and_then(|value| value.to_fragment_binding())
    }

    pub(crate) fn definitely_nonempty_iterable(&self) -> bool {
        AbstractValue::from_fragment_binding(self).definitely_nonempty_iterable()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::FragmentBinding;

    #[test]
    fn values_root_abstains_from_fragment_path_extraction() {
        assert_eq!(FragmentBinding::ValuesRoot.paths(), BTreeSet::new());
        assert_eq!(
            FragmentBinding::ValuesRoot.rendered_paths(),
            BTreeSet::new()
        );
    }

    #[test]
    fn fragment_paths_stay_shallow_while_rendered_paths_descend_structures() {
        let binding = FragmentBinding::Dict(BTreeMap::from([(
            "metadata".to_string(),
            FragmentBinding::ValuesPath("podLabels".to_string()),
        )]));

        assert_eq!(binding.paths(), BTreeSet::new());
        assert_eq!(
            binding.rendered_paths(),
            BTreeSet::from(["podLabels".to_string()])
        );
    }
}
