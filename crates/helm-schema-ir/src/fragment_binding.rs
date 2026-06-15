use std::collections::{BTreeMap, BTreeSet};

use crate::helper_analysis::HelperOutputMeta;
use crate::helper_binding::HelperBinding;
use crate::{ValueKind, YamlPath};

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
        match self {
            Self::ValuesPath(path) => Some(HelperBinding::ValuesPath(path.clone())),
            Self::ValuesRoot => Some(HelperBinding::ValuesPath(String::new())),
            Self::RootContext => Some(HelperBinding::RootContext),
            Self::Unknown => Some(HelperBinding::Unknown),
            Self::StringSet(strings) => Some(HelperBinding::StringSet(strings.clone())),
            Self::OutputSet(paths) => Some(HelperBinding::OutputSet(
                paths
                    .iter()
                    .map(|path| (path.clone(), HelperOutputMeta::default()))
                    .collect(),
            )),
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
            Self::Choice(choices) => {
                HelperBinding::choice(choices.iter().filter_map(Self::to_helper_binding).collect())
            }
        }
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
        match self {
            Self::ValuesPath(path) => [path.clone()].into_iter().collect(),
            Self::OutputSet(paths) => paths.clone(),
            Self::PathSet(paths) => paths.clone(),
            Self::Overlay { entries, fallback } => entries
                .values()
                .flat_map(Self::paths)
                .chain(fallback.paths())
                .collect(),
            Self::Choice(choices) => choices.iter().flat_map(Self::paths).collect(),
            _ => BTreeSet::new(),
        }
    }

    pub(crate) fn rendered_paths(&self) -> BTreeSet<String> {
        match self {
            Self::ValuesPath(path) => [path.clone()].into_iter().collect(),
            Self::OutputSet(paths) => paths.clone(),
            Self::PathSet(paths) => paths.clone(),
            Self::Dict(map) => map.values().flat_map(Self::rendered_paths).collect(),
            Self::List(items) => items.iter().flat_map(Self::rendered_paths).collect(),
            Self::Overlay { entries, fallback } => entries
                .values()
                .flat_map(Self::rendered_paths)
                .chain(fallback.rendered_paths())
                .collect(),
            Self::Choice(choices) => choices.iter().flat_map(Self::rendered_paths).collect(),
            _ => BTreeSet::new(),
        }
    }

    pub(crate) fn strings(&self) -> BTreeSet<String> {
        match self {
            Self::StringSet(values) => values.clone(),
            Self::Choice(choices) => choices.iter().flat_map(Self::strings).collect(),
            _ => BTreeSet::new(),
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

    pub(crate) fn for_output_path(source_expr: String, relative_path: &YamlPath) -> Self {
        let mut binding = Self::OutputSet([source_expr].into_iter().collect());
        for segment in relative_path.0.iter().rev() {
            binding = Self::Dict(BTreeMap::from([(segment.clone(), binding)]));
        }
        binding
    }

    pub(crate) fn apply_to_binding(&self, rest: &[String]) -> Option<Self> {
        match self {
            Self::ValuesPath(prefix) => {
                if rest.is_empty() {
                    Some(Self::ValuesPath(prefix.clone()))
                } else if prefix.is_empty() {
                    Some(Self::ValuesPath(rest.join(".")))
                } else {
                    Some(Self::ValuesPath(format!("{prefix}.{}", rest.join("."))))
                }
            }
            Self::ValuesRoot => {
                if rest.is_empty() {
                    Some(Self::ValuesRoot)
                } else {
                    Some(Self::ValuesPath(rest.join(".")))
                }
            }
            Self::RootContext => match rest {
                [head] if head == "Values" => Some(Self::ValuesRoot),
                [head, tail @ ..] if head == "Values" => Some(Self::ValuesPath(tail.join("."))),
                _ => None,
            },
            Self::Unknown => None,
            Self::Dict(map) => {
                let (first, tail) = rest.split_first()?;
                let binding = map.get(first)?;
                if tail.is_empty() {
                    Some(binding.clone())
                } else {
                    binding.apply_to_binding(tail)
                }
            }
            Self::PathSet(paths) => {
                let appended: BTreeSet<String> = paths
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
            Self::OutputSet(paths) => Some(Self::OutputSet(paths.clone())),
            Self::Overlay { entries, fallback } => {
                let (first, tail) = rest.split_first()?;
                if let Some(binding) = entries.get(first) {
                    if tail.is_empty() {
                        Some(binding.clone())
                    } else {
                        binding.apply_to_binding(tail)
                    }
                } else {
                    fallback.apply_to_binding(rest)
                }
            }
            Self::Choice(choices) => Self::choice(
                choices
                    .iter()
                    .filter_map(|choice| choice.apply_to_binding(rest))
                    .collect(),
            ),
            Self::List(_) | Self::StringSet(_) => None,
        }
    }

    pub(crate) fn item_binding(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => Some(Self::ValuesPath(format!("{path}.*"))),
            Self::ValuesRoot => Some(Self::ValuesPath("*".to_string())),
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
            Self::OutputSet(paths) => Some(Self::OutputSet(paths.clone())),
            Self::List(items) => Self::choice(items.clone()),
            Self::Choice(choices) => {
                Self::choice(choices.iter().filter_map(Self::item_binding).collect())
            }
            Self::RootContext
            | Self::Unknown
            | Self::Dict(_)
            | Self::Overlay { .. }
            | Self::StringSet(_) => None,
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

    pub(crate) fn output_child_kind(&self) -> ValueKind {
        match self {
            Self::Dict(_) | Self::List(_) | Self::Overlay { .. } => ValueKind::Fragment,
            Self::Choice(choices)
                if choices.iter().any(|choice| {
                    matches!(choice, Self::Dict(_) | Self::List(_) | Self::Overlay { .. })
                }) =>
            {
                ValueKind::Fragment
            }
            Self::ValuesPath(_)
            | Self::ValuesRoot
            | Self::RootContext
            | Self::Unknown
            | Self::StringSet(_)
            | Self::PathSet(_)
            | Self::OutputSet(_)
            | Self::Choice(_) => ValueKind::Scalar,
        }
    }
}
