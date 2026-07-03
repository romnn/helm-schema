use std::collections::{BTreeMap, BTreeSet};

use crate::helper_meta::HelperOutputMeta;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AbstractValue {
    Top,
    Unknown,
    ValuesPath(String),
    OutputPath(String, HelperOutputMeta),
    RootContext,
    StringSet(BTreeSet<String>),
    Dict(BTreeMap<String, AbstractValue>),
    List(Vec<AbstractValue>),
    Overlay {
        entries: BTreeMap<String, AbstractValue>,
        fallback: Box<AbstractValue>,
    },
    Choice(BTreeSet<AbstractValue>),
    /// Result of a call without a transfer function: the value itself is
    /// unknown (structural operations treat it like `Unknown`), but the
    /// `.Values` paths that flowed into the call are kept so output
    /// projection can still attribute the rendered text to its sources.
    /// Declared last so projected rows sort after structured alternatives.
    Widened(BTreeSet<String>),
}

impl AbstractValue {
    pub(crate) fn values_root() -> Self {
        Self::ValuesPath(String::new())
    }

    pub(crate) fn paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        self.collect_paths(&mut paths, true, false);
        paths
    }

    fn collect_paths(
        &self,
        out: &mut BTreeSet<String>,
        descend_structures: bool,
        suppress_values_root: bool,
    ) {
        match self {
            Self::ValuesPath(path) => {
                if !suppress_values_root || !path.is_empty() {
                    out.insert(path.clone());
                }
            }
            Self::OutputPath(path, _) => {
                out.insert(path.clone());
            }
            Self::Dict(map) if descend_structures => {
                for value in map.values() {
                    value.collect_paths(out, descend_structures, suppress_values_root);
                }
            }
            Self::List(items) if descend_structures => {
                for value in items {
                    value.collect_paths(out, descend_structures, suppress_values_root);
                }
            }
            Self::Overlay { entries, fallback } => entries
                .values()
                .chain(std::iter::once(fallback.as_ref()))
                .for_each(|value| {
                    value.collect_paths(out, descend_structures, suppress_values_root);
                }),
            Self::Choice(choices) => {
                for value in choices {
                    value.collect_paths(out, descend_structures, suppress_values_root);
                }
            }
            // Influence paths surface in full path collection (output
            // attribution), but a widened value is not a fragment source:
            // fragment projections must treat it like `Unknown`.
            Self::Widened(paths) => {
                if !suppress_values_root {
                    out.extend(paths.iter().cloned());
                }
            }
            Self::Top
            | Self::Unknown
            | Self::RootContext
            | Self::StringSet(_)
            | Self::Dict(_)
            | Self::List(_) => {}
        }
    }

    pub(crate) fn strings(&self) -> BTreeSet<String> {
        match self {
            Self::StringSet(strings) => strings.clone(),
            Self::Choice(choices) => choices.iter().flat_map(Self::strings).collect(),
            _ => BTreeSet::new(),
        }
    }

    pub(crate) fn fragment_range_item(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => Some(Self::ValuesPath(item_path(path))),
            Self::OutputPath(path, meta) => Some(Self::OutputPath(path.clone(), meta.clone())),
            Self::List(items) => Self::choice(items.clone()),
            Self::Choice(choices) => Self::choice(
                choices
                    .iter()
                    .filter_map(Self::fragment_range_item)
                    .collect(),
            ),
            Self::Top
            | Self::Unknown
            | Self::RootContext
            | Self::StringSet(_)
            | Self::Dict(_)
            | Self::Overlay { .. }
            | Self::Widened(_) => None,
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

    pub(crate) fn choice(values: Vec<Self>) -> Option<Self> {
        Self::join_all(values)
    }

    pub(crate) fn path_choices(paths: BTreeSet<String>) -> Option<Self> {
        Self::choice(paths.into_iter().map(Self::ValuesPath).collect())
    }

    pub(crate) fn widened(paths: BTreeSet<String>) -> Option<Self> {
        if paths.is_empty() {
            None
        } else {
            Some(Self::Widened(paths))
        }
    }

    /// A widened value flows to output projection, but it is not a
    /// values-backed fragment: binding it to a local must behave like the
    /// unknown call results did before provenance carrying, i.e. not bind.
    pub(crate) fn without_widened(self) -> Option<Self> {
        match self {
            Self::Widened(_) => None,
            Self::Choice(choices) => Self::choice(
                choices
                    .into_iter()
                    .filter_map(Self::without_widened)
                    .collect(),
            ),
            other => Some(other),
        }
    }

    pub(crate) fn join_all(values: Vec<Self>) -> Option<Self> {
        let mut flat = BTreeSet::new();
        let mut pending = values;
        while let Some(value) = pending.pop() {
            match value {
                Self::Choice(inner) => pending.extend(inner),
                Self::Unknown => {
                    flat.insert(Self::Top);
                }
                other => {
                    flat.insert(other);
                }
            }
        }
        // An unknown alternative widens the join but must not erase the
        // structured alternatives: path attribution has to survive joins
        // such as `default $unknown .Values.x`. A single Top member records
        // the width.
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
            Self::OutputPath(prefix, meta) => Some(Self::OutputPath(prefix.clone(), meta.clone())),
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
            Self::Top => Some(Self::Top),
            // Selecting into an unknown call result severs the influence:
            // the selected member is not derived from the recorded paths in
            // any way the projection could still attribute.
            Self::Unknown | Self::Widened(_) => None,
            Self::StringSet(_) => None,
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

    /// Merges `entries` into `map`, joining values that land on an existing
    /// key. Both merge folds share this per-key rule.
    fn merge_entries(map: &mut BTreeMap<String, Self>, entries: BTreeMap<String, Self>) {
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

    pub(crate) fn merge_all(values: Vec<Self>) -> Option<Self> {
        let mut map = BTreeMap::new();
        let mut non_dict_values = Vec::new();
        let mut pending = values;

        while let Some(value) = pending.pop() {
            match value {
                Self::Choice(choices) => pending.extend(choices),
                Self::Dict(entries) => Self::merge_entries(&mut map, entries),
                // Top/Unknown deliberately survive as fallback members here,
                // unlike merge_context_values, which keeps only values-backed
                // members.
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

    pub(crate) fn with_overlay_entries(self, new_entries: BTreeMap<String, AbstractValue>) -> Self {
        if new_entries.is_empty() {
            return self;
        }
        match self {
            Self::Overlay {
                mut entries,
                fallback,
            } => {
                entries.extend(new_entries);
                Self::Overlay { entries, fallback }
            }
            other => Self::Overlay {
                entries: new_entries,
                fallback: Box::new(other),
            },
        }
    }

    pub(crate) fn omit_keys(self, keys: &BTreeSet<String>) -> Self {
        if keys.is_empty() {
            return self;
        }

        match self {
            Self::Dict(entries) => Self::Dict(
                entries
                    .into_iter()
                    .filter(|(key, _value)| !keys.contains(key))
                    .collect(),
            ),
            Self::Overlay { entries, fallback } => Self::Overlay {
                entries: entries
                    .into_iter()
                    .filter(|(key, _value)| !keys.contains(key))
                    .collect(),
                fallback: Box::new(fallback.omit_keys(keys)),
            },
            Self::Choice(choices) => Self::Choice(
                choices
                    .into_iter()
                    .map(|choice| choice.omit_keys(keys))
                    .collect(),
            ),
            other => other,
        }
    }

    pub(crate) fn remove_fragment_paths(self, remove: &BTreeSet<String>) -> Option<Self> {
        if remove.is_empty() {
            return Some(self);
        }

        match self {
            Self::ValuesPath(path) if remove.contains(&path) => None,
            Self::OutputPath(path, _) if remove.contains(&path) => None,
            Self::ValuesPath(_)
            | Self::OutputPath(_, _)
            | Self::RootContext
            | Self::Unknown
            | Self::Top
            | Self::StringSet(_) => Some(self),
            Self::Widened(paths) => Self::widened(paths.difference(remove).cloned().collect()),
            Self::Dict(entries) => {
                let entries = Self::remove_fragment_paths_from_entries(entries, remove);
                if entries.is_empty() {
                    None
                } else {
                    Some(Self::Dict(entries))
                }
            }
            Self::List(items) => {
                let items = items
                    .into_iter()
                    .filter_map(|item| item.remove_fragment_paths(remove))
                    .collect::<Vec<_>>();
                if items.is_empty() {
                    None
                } else {
                    Some(Self::List(items))
                }
            }
            Self::Overlay { entries, fallback } => {
                let entries = Self::remove_fragment_paths_from_entries(entries, remove);
                match (entries.is_empty(), fallback.remove_fragment_paths(remove)) {
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
                    .filter_map(|choice| choice.remove_fragment_paths(remove))
                    .collect(),
            ),
        }
    }

    fn remove_fragment_paths_from_entries(
        entries: BTreeMap<String, Self>,
        remove: &BTreeSet<String>,
    ) -> BTreeMap<String, Self> {
        entries
            .into_iter()
            .filter_map(|(key, value)| {
                value
                    .remove_fragment_paths(remove)
                    .map(|value| (key, value))
            })
            .collect()
    }

    pub(crate) fn to_context_value(&self) -> Self {
        match self {
            Self::Top => Self::Unknown,
            other => other.clone(),
        }
    }

    pub(crate) fn to_current_dot_context_value(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => Some(Self::ValuesPath(path.clone())),
            Self::OutputPath(path, meta) => Some(Self::OutputPath(path.clone(), meta.clone())),
            Self::RootContext => Some(Self::RootContext),
            Self::Top
            | Self::Unknown
            | Self::Dict(_)
            | Self::List(_)
            | Self::Overlay { .. }
            | Self::StringSet(_)
            | Self::Choice(_)
            | Self::Widened(_) => None,
        }
    }

    pub(crate) fn fragment_source_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        self.collect_paths(&mut paths, false, true);
        paths
    }

    pub(crate) fn fragment_rendered_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        self.collect_paths(&mut paths, true, true);
        paths
    }
}

fn item_path(path: &str) -> String {
    if path.is_empty() {
        "*".to_string()
    } else {
        format!("{path}.*")
    }
}

pub(crate) fn path_is_encoded(path: &str, encoded_paths: &BTreeSet<String>) -> bool {
    encoded_paths.iter().any(|encoded_path| {
        path == encoded_path
            || path
                .strip_prefix(encoded_path)
                .is_some_and(|suffix| suffix.starts_with('.'))
    })
}

#[cfg(test)]
#[path = "tests/abstract_value.rs"]
mod tests;
