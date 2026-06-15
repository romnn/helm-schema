use std::collections::{BTreeMap, BTreeSet};

use crate::fragment_binding::FragmentBinding;
use crate::helper_analysis::HelperOutputMeta;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HelperBinding {
    ValuesPath(String),
    RootContext,
    Unknown,
    OutputSet(BTreeMap<String, HelperOutputMeta>),
    StringSet(BTreeSet<String>),
    PathSet(BTreeSet<String>),
    Dict(BTreeMap<String, HelperBinding>),
    List(Vec<HelperBinding>),
    Overlay {
        entries: BTreeMap<String, HelperBinding>,
        fallback: Box<HelperBinding>,
    },
    Choice(BTreeSet<HelperBinding>),
}

impl HelperBinding {
    pub(crate) fn to_fragment_binding(&self) -> FragmentBinding {
        match self {
            Self::ValuesPath(path) => FragmentBinding::ValuesPath(path.clone()),
            Self::RootContext => FragmentBinding::RootContext,
            Self::Unknown => FragmentBinding::Unknown,
            Self::OutputSet(outputs) => {
                FragmentBinding::OutputSet(outputs.keys().cloned().collect())
            }
            Self::StringSet(strings) => FragmentBinding::StringSet(strings.clone()),
            Self::PathSet(paths) => FragmentBinding::PathSet(paths.clone()),
            Self::Dict(map) => FragmentBinding::Dict(
                map.iter()
                    .map(|(key, value)| (key.clone(), value.to_fragment_binding()))
                    .collect(),
            ),
            Self::List(items) => {
                FragmentBinding::List(items.iter().map(Self::to_fragment_binding).collect())
            }
            Self::Overlay { entries, fallback } => FragmentBinding::Overlay {
                entries: entries
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_fragment_binding()))
                    .collect(),
                fallback: Box::new(fallback.to_fragment_binding()),
            },
            Self::Choice(choices) => {
                FragmentBinding::Choice(choices.iter().map(Self::to_fragment_binding).collect())
            }
        }
    }

    pub(crate) fn paths(&self) -> BTreeSet<String> {
        match self {
            Self::ValuesPath(path) => [path.clone()].into_iter().collect(),
            Self::OutputSet(outputs) => outputs.keys().cloned().collect(),
            Self::PathSet(paths) => paths.clone(),
            Self::Dict(map) => map.values().flat_map(Self::paths).collect(),
            Self::List(items) => items.iter().flat_map(Self::paths).collect(),
            Self::Overlay { entries, fallback } => entries
                .values()
                .flat_map(Self::paths)
                .chain(fallback.paths())
                .collect(),
            Self::Choice(choices) => choices.iter().flat_map(Self::paths).collect(),
            Self::RootContext | Self::Unknown | Self::StringSet(_) => BTreeSet::new(),
        }
    }

    pub(crate) fn strings(&self) -> BTreeSet<String> {
        match self {
            Self::StringSet(strings) => strings.clone(),
            Self::Choice(choices) => choices.iter().flat_map(Self::strings).collect(),
            _ => BTreeSet::new(),
        }
    }

    pub(crate) fn item_binding(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => {
                if path.is_empty() {
                    Some(Self::ValuesPath("*".to_string()))
                } else {
                    Some(Self::ValuesPath(format!("{path}.*")))
                }
            }
            Self::PathSet(paths) => Some(Self::PathSet(
                paths
                    .iter()
                    .map(|path| {
                        if path.is_empty() {
                            "*".to_string()
                        } else {
                            format!("{path}.*")
                        }
                    })
                    .collect(),
            )),
            Self::OutputSet(outputs) => Some(Self::OutputSet(
                outputs
                    .iter()
                    .map(|(path, meta)| (path.clone(), meta.clone()))
                    .collect(),
            )),
            Self::Dict(entries) => Self::choice(entries.values().cloned().collect()),
            Self::List(items) => Self::choice(items.clone()),
            Self::Overlay { entries, fallback } => {
                let mut choices: Vec<_> = entries.values().cloned().collect();
                if let Some(fallback_item) = fallback.item_binding() {
                    choices.push(fallback_item);
                }
                Self::choice(choices)
            }
            Self::Choice(choices) => {
                Self::choice(choices.iter().filter_map(Self::item_binding).collect())
            }
            Self::RootContext | Self::Unknown | Self::StringSet(_) => None,
        }
    }

    pub(crate) fn definitely_nonempty_iterable(&self) -> bool {
        match self {
            Self::List(items) => !items.is_empty(),
            Self::Choice(choices) => {
                !choices.is_empty() && choices.iter().all(Self::definitely_nonempty_iterable)
            }
            _ => false,
        }
    }

    pub(crate) fn choice(bindings: Vec<Self>) -> Option<Self> {
        let mut choices = BTreeSet::new();
        for binding in bindings {
            match binding {
                Self::Choice(inner) => choices.extend(inner),
                Self::Unknown => {}
                other => {
                    choices.insert(other);
                }
            }
        }
        match choices.len() {
            0 => None,
            1 => choices.into_iter().next(),
            _ => Some(Self::Choice(choices)),
        }
    }

    pub(crate) fn merge_all(bindings: Vec<Self>) -> Option<Self> {
        let mut map = BTreeMap::new();
        let mut non_dict_bindings = Vec::new();

        let mut pending = bindings;
        while let Some(binding) = pending.pop() {
            match binding {
                Self::Choice(choices) => {
                    pending.extend(choices);
                }
                Self::Dict(entries) => {
                    for (key, value) in entries {
                        let merged = match map.remove(&key) {
                            Some(existing) => Self::choice(vec![existing, value]),
                            None => Some(value),
                        };
                        if let Some(merged) = merged {
                            map.insert(key, merged);
                        }
                    }
                }
                other => {
                    non_dict_bindings.push(other);
                }
            }
        }

        let fallback = Self::choice(non_dict_bindings);
        match (map.is_empty(), fallback) {
            (true, None) => None,
            (false, None) => Some(Self::Dict(map)),
            (true, Some(fallback)) => Some(fallback),
            (false, Some(fallback)) => Some(Self::Overlay {
                entries: map,
                fallback: Box::new(fallback),
            }),
        }
    }
}
