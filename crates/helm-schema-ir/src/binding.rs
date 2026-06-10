use std::collections::{BTreeMap, BTreeSet};

use crate::helper_analysis::HelperOutputMeta;
use crate::{ValueKind, YamlPath};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct StaticFileTemplate {
    pub(crate) path: String,
    pub(crate) dot: Option<FragmentBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HelperBinding {
    ValuesPath(String),
    RootContext,
    Unknown,
    OutputSet(BTreeMap<String, HelperOutputMeta>),
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
    pub(crate) fn for_output_path(
        source_expr: String,
        relative_path: &YamlPath,
        meta: HelperOutputMeta,
    ) -> Self {
        let mut binding = Self::OutputSet(BTreeMap::from([(source_expr, meta)]));
        for segment in relative_path.0.iter().rev() {
            binding = Self::Dict(BTreeMap::from([(segment.clone(), binding)]));
        }
        binding
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
            Self::RootContext | Self::Unknown => BTreeSet::new(),
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
            Self::RootContext | Self::Unknown => None,
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

    pub(crate) fn apply_to_binding(&self, rest: &[String]) -> Option<Self> {
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
            Self::RootContext => match rest {
                [head] if head == "Values" => Some(Self::ValuesPath(String::new())),
                [head, tail @ ..] if head == "Values" => Some(Self::ValuesPath(tail.join("."))),
                _ => None,
            },
            Self::Unknown => None,
            Self::OutputSet(outputs) => Some(Self::OutputSet(
                outputs
                    .iter()
                    .map(|(path, meta)| (path.clone(), meta.clone()))
                    .collect(),
            )),
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
            Self::Dict(map) => {
                let (head, tail) = rest.split_first()?;
                let binding = map.get(head)?;
                binding.apply_to_binding(tail)
            }
            Self::List(items) => {
                let (head, tail) = rest.split_first()?;
                let index = head.parse::<usize>().ok()?;
                let binding = items.get(index)?;
                binding.apply_to_binding(tail)
            }
            Self::Overlay { entries, fallback } => {
                let (head, tail) = rest.split_first()?;
                if let Some(binding) = entries.get(head) {
                    binding.apply_to_binding(tail)
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
        }
    }

    pub(crate) fn apply_unique_path(&self, rest: &[String]) -> Option<String> {
        let binding = self.apply_to_binding(rest)?;
        let paths = binding.paths();
        let mut paths = paths.into_iter();
        let first = paths.next()?;
        if paths.next().is_none() {
            Some(first)
        } else {
            None
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
            | Self::RootContext
            | Self::Unknown
            | Self::OutputSet(_)
            | Self::PathSet(_)
            | Self::Choice(_) => ValueKind::Scalar,
        }
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct BoundHelperCallsCacheKey {
    pub(crate) text: String,
    pub(crate) current_dot: Option<HelperBinding>,
    pub(crate) root_bindings: BTreeMap<String, HelperBinding>,
    pub(crate) fragment_locals: BTreeMap<String, FragmentBinding>,
}
