use std::collections::{BTreeMap, BTreeSet};

use crate::binding::HelperBinding;
use crate::helper_analysis::HelperOutputMeta;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AbstractValue {
    Unknown,
    ValuesPath(String),
    RootContext,
    OutputSet(BTreeMap<String, HelperOutputMeta>),
    PathSet(BTreeSet<String>),
    Dict(BTreeMap<String, AbstractValue>),
    List(Vec<AbstractValue>),
    Overlay {
        entries: BTreeMap<String, AbstractValue>,
        fallback: Box<AbstractValue>,
    },
    Choice(BTreeSet<AbstractValue>),
}

impl AbstractValue {
    pub(crate) fn values_root() -> Self {
        Self::ValuesPath(String::new())
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
            Self::Unknown | Self::RootContext => BTreeSet::new(),
        }
    }

    pub(crate) fn choice(values: Vec<Self>) -> Option<Self> {
        let mut flat = BTreeSet::new();
        for value in values {
            match value {
                Self::Choice(inner) => flat.extend(inner),
                Self::Unknown => {}
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

    pub(crate) fn apply_to_path(&self, rest: &[String]) -> Option<Self> {
        if rest.is_empty() {
            return Some(self.clone());
        }

        match self {
            Self::ValuesPath(prefix) => {
                if prefix.is_empty() {
                    Some(Self::ValuesPath(rest.join(".")))
                } else {
                    Some(Self::ValuesPath(format!("{prefix}.{}", rest.join("."))))
                }
            }
            Self::RootContext => {
                if rest.first().is_some_and(|segment| segment == "Values") {
                    if rest.len() == 1 {
                        Some(Self::values_root())
                    } else {
                        Some(Self::ValuesPath(rest[1..].join(".")))
                    }
                } else {
                    None
                }
            }
            Self::Unknown => None,
            Self::OutputSet(outputs) => Some(Self::OutputSet(outputs.clone())),
            Self::PathSet(paths) => {
                let appended = paths
                    .iter()
                    .map(|path| {
                        if rest.is_empty() {
                            path.clone()
                        } else if path.is_empty() {
                            rest.join(".")
                        } else {
                            format!("{path}.{}", rest.join("."))
                        }
                    })
                    .collect();
                Some(Self::PathSet(appended))
            }
            Self::Choice(choices) => {
                let mut out = Vec::new();
                for value in choices {
                    if let Some(bound) = value.apply_to_path(rest) {
                        out.push(bound);
                    }
                }
                Self::choice(out)
            }
            Self::Dict(map) => {
                let (head, tail) = rest.split_first()?;
                let value = map.get(head)?;
                value.apply_to_path(tail)
            }
            Self::List(items) => {
                let (head, tail) = rest.split_first()?;
                let index = head.parse::<usize>().ok()?;
                let value = items.get(index)?;
                value.apply_to_path(tail)
            }
            Self::Overlay { entries, fallback } => {
                let (head, tail) = rest.split_first()?;
                if let Some(value) = entries.get(head) {
                    value.apply_to_path(tail)
                } else {
                    fallback.apply_to_path(rest)
                }
            }
        }
    }

    pub(crate) fn item(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => {
                if path.is_empty() {
                    Some(Self::ValuesPath("*".to_string()))
                } else {
                    Some(Self::ValuesPath(format!("{path}.*")))
                }
            }
            Self::RootContext => None,
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
            Self::OutputSet(outputs) => Some(Self::OutputSet(outputs.clone())),
            Self::Choice(choices) => {
                let mut out = Vec::new();
                for choice_value in choices {
                    if let Some(bound) = choice_value.item() {
                        out.push(bound);
                    }
                }
                Self::choice(out)
            }
            Self::List(items) => Self::choice(items.clone()),
            Self::Dict(map) => Self::choice(map.values().cloned().collect()),
            Self::Overlay { entries, fallback } => {
                let mut choices: Vec<_> = entries.values().cloned().collect();
                if let Some(fallback_item) = fallback.item() {
                    choices.push(fallback_item);
                }
                Self::choice(choices)
            }
            Self::Unknown => None,
        }
    }

    pub(crate) fn merge_all(values: Vec<Self>) -> Option<Self> {
        let mut map = BTreeMap::new();
        let mut non_dict_values = Vec::new();
        let mut pending = values;

        while let Some(value) = pending.pop() {
            match value {
                Self::Choice(choices) => pending.extend(choices),
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
                Self::Unknown => {}
                other => non_dict_values.push(other),
            }
        }

        let fallback = Self::choice(non_dict_values);
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

    pub(crate) fn unique_path(&self) -> Option<String> {
        let mut paths = self.paths().into_iter();
        let first = paths.next()?;
        if paths.next().is_none() {
            Some(first)
        } else {
            None
        }
    }

    pub(crate) fn from_helper_binding(binding: &HelperBinding) -> Self {
        match binding {
            HelperBinding::ValuesPath(path) => Self::ValuesPath(path.clone()),
            HelperBinding::RootContext => Self::RootContext,
            HelperBinding::Unknown => Self::Unknown,
            HelperBinding::OutputSet(outputs) => Self::OutputSet(outputs.clone()),
            HelperBinding::PathSet(paths) => Self::PathSet(paths.clone()),
            HelperBinding::Dict(map) => Self::Dict(
                map.iter()
                    .map(|(key, value)| (key.clone(), Self::from_helper_binding(value)))
                    .collect(),
            ),
            HelperBinding::List(items) => {
                Self::List(items.iter().map(Self::from_helper_binding).collect())
            }
            HelperBinding::Overlay { entries, fallback } => Self::Overlay {
                entries: entries
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from_helper_binding(value)))
                    .collect(),
                fallback: Box::new(Self::from_helper_binding(fallback)),
            },
            HelperBinding::Choice(choices) => Self::Choice(
                choices
                    .iter()
                    .map(Self::from_helper_binding)
                    .collect::<BTreeSet<_>>(),
            ),
        }
    }

    pub(crate) fn to_helper_binding(&self) -> Option<HelperBinding> {
        match self {
            Self::ValuesPath(path) => Some(HelperBinding::ValuesPath(path.clone())),
            Self::RootContext => Some(HelperBinding::RootContext),
            Self::Unknown => Some(HelperBinding::Unknown),
            Self::OutputSet(outputs) => Some(HelperBinding::OutputSet(outputs.clone())),
            Self::PathSet(paths) => Some(HelperBinding::PathSet(paths.clone())),
            Self::Dict(map) => Some(HelperBinding::Dict(
                map.iter()
                    .map(|(key, value)| Some((key.clone(), value.to_helper_binding()?)))
                    .collect::<Option<BTreeMap<_, _>>>()?,
            )),
            Self::List(items) => Some(HelperBinding::List(
                items
                    .iter()
                    .map(Self::to_helper_binding)
                    .collect::<Option<Vec<_>>>()?,
            )),
            Self::Overlay { entries, fallback } => Some(HelperBinding::Overlay {
                entries: entries
                    .iter()
                    .map(|(key, value)| Some((key.clone(), value.to_helper_binding()?)))
                    .collect::<Option<BTreeMap<_, _>>>()?,
                fallback: Box::new(fallback.to_helper_binding()?),
            }),
            Self::Choice(choices) => Some(HelperBinding::Choice(
                choices
                    .iter()
                    .map(Self::to_helper_binding)
                    .collect::<Option<BTreeSet<_>>>()?,
            )),
        }
    }
}
