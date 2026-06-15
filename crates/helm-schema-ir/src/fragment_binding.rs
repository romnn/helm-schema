use std::collections::{BTreeMap, BTreeSet};

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
}
