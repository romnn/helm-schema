use std::collections::{BTreeMap, BTreeSet};

use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::{ValueKind, YamlPath};
use helm_schema_core::{self as output_path, Predicate};

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
        self.range_item(false)
    }

    fn range_item(&self, include_map_values: bool) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => Some(Self::ValuesPath(item_path(path))),
            Self::OutputPath(path, meta) => Some(Self::OutputPath(path.clone(), meta.clone())),
            Self::Dict(entries) if include_map_values => {
                Self::choice(entries.values().cloned().collect())
            }
            Self::List(items) => Self::choice(items.clone()),
            Self::Overlay { entries, fallback } if include_map_values => {
                let mut choices: Vec<_> = entries.values().cloned().collect();
                if let Some(fallback_item) = fallback.range_item(include_map_values) {
                    choices.push(fallback_item);
                }
                Self::choice(choices)
            }
            Self::Choice(choices) => Self::choice(
                choices
                    .iter()
                    .filter_map(|choice| choice.range_item(include_map_values))
                    .collect(),
            ),
            Self::Top
            | Self::Unknown
            | Self::RootContext
            | Self::StringSet(_)
            | Self::Dict(_)
            | Self::Overlay { .. } => None,
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

    pub(crate) fn join_all(values: Vec<Self>) -> Option<Self> {
        let mut flat = BTreeSet::new();
        for value in values {
            match value {
                Self::Top | Self::Unknown => return Some(Self::Top),
                Self::Choice(inner) => {
                    for choice in inner {
                        match choice {
                            Self::Top | Self::Unknown => return Some(Self::Top),
                            other => {
                                flat.insert(other);
                            }
                        }
                    }
                }
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
            Self::Unknown => None,
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
                Self::Top => {
                    non_dict_values.push(Self::Top);
                }
                Self::Unknown => {
                    non_dict_values.push(Self::Unknown);
                }
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

    pub(crate) fn merge_context_values(values: Vec<Self>) -> Option<Self> {
        let mut map = BTreeMap::new();
        let mut non_dict_value_paths = BTreeSet::new();
        let mut non_dict_output_paths = Vec::new();
        let mut non_dict_strings = BTreeSet::new();

        let mut pending = values;
        while let Some(binding) = pending.pop() {
            match binding {
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
                Self::Overlay { entries, fallback } => {
                    pending.push(*fallback);
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
                Self::ValuesPath(path) => {
                    non_dict_value_paths.insert(path);
                }
                Self::OutputPath(path, meta) => {
                    non_dict_output_paths.push(Self::OutputPath(path, meta));
                }
                Self::StringSet(strings) => {
                    non_dict_strings.extend(strings);
                }
                Self::RootContext | Self::Unknown | Self::Top | Self::List(_) => {}
            }
        }

        let mut fallback_choices = Vec::new();
        if let Some(paths) = Self::path_choices(non_dict_value_paths) {
            fallback_choices.push(paths);
        }
        fallback_choices.extend(non_dict_output_paths);
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
            Self::Dict(entries) => {
                let entries = entries
                    .into_iter()
                    .filter_map(|(key, value)| {
                        value
                            .remove_fragment_paths(remove)
                            .map(|value| (key, value))
                    })
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
                    .filter_map(|item| item.remove_fragment_paths(remove))
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
                    .filter_map(|(key, value)| {
                        value
                            .remove_fragment_paths(remove)
                            .map(|value| (key, value))
                    })
                    .collect::<BTreeMap<_, _>>();
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
            | Self::Choice(_) => None,
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

    pub(crate) fn collect_output_uses_with_encoding(
        &self,
        outputs: &mut Vec<HelperFragmentOutputUse>,
        relative_path: &YamlPath,
        kind: ValueKind,
        encoded_paths: &BTreeSet<String>,
        active_output_predicates: &BTreeSet<Predicate>,
        defaulted_paths: &BTreeSet<String>,
        suppress_values_root: bool,
    ) {
        match self {
            Self::ValuesPath(path) if !suppress_values_root || !path.is_empty() => {
                push_output_path(
                    outputs,
                    path,
                    relative_path,
                    kind,
                    None,
                    encoded_paths,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
            Self::ValuesPath(_) => {}
            Self::OutputPath(path, meta) => {
                push_output_path(
                    outputs,
                    path,
                    relative_path,
                    kind,
                    Some(meta),
                    encoded_paths,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
            Self::Dict(entries) => {
                for (key, value) in entries {
                    let child_path = output_path::append_relative_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    value.collect_output_uses_with_encoding(
                        outputs,
                        &child_path,
                        value.output_child_kind(),
                        encoded_paths,
                        active_output_predicates,
                        defaulted_paths,
                        suppress_values_root,
                    );
                }
            }
            Self::Overlay { entries, fallback } => {
                fallback.collect_output_uses_with_encoding(
                    outputs,
                    relative_path,
                    kind,
                    encoded_paths,
                    active_output_predicates,
                    defaulted_paths,
                    suppress_values_root,
                );
                for (key, value) in entries {
                    let child_path = output_path::append_relative_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    value.collect_output_uses_with_encoding(
                        outputs,
                        &child_path,
                        value.output_child_kind(),
                        encoded_paths,
                        active_output_predicates,
                        defaulted_paths,
                        suppress_values_root,
                    );
                }
            }
            Self::Choice(choices) => {
                for choice in choices {
                    choice.collect_output_uses_with_encoding(
                        outputs,
                        relative_path,
                        kind,
                        encoded_paths,
                        active_output_predicates,
                        defaulted_paths,
                        suppress_values_root,
                    );
                }
            }
            Self::List(items) => {
                let item_path = output_path::sequence_item_path(relative_path);
                for item in items {
                    item.collect_output_uses_with_encoding(
                        outputs,
                        &item_path,
                        item.output_child_kind(),
                        encoded_paths,
                        active_output_predicates,
                        defaulted_paths,
                        suppress_values_root,
                    );
                }
            }
            Self::Top | Self::Unknown | Self::RootContext | Self::StringSet(_) => {}
        }
    }

    pub(crate) fn for_output_path(
        source_expr: String,
        relative_path: &YamlPath,
        meta: HelperOutputMeta,
    ) -> Self {
        let mut value = Self::OutputPath(source_expr, meta);
        for segment in relative_path.0.iter().rev() {
            value = Self::Dict(BTreeMap::from([(segment.clone(), value)]));
        }
        value
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
            Self::Top
            | Self::Unknown
            | Self::ValuesPath(_)
            | Self::OutputPath(_, _)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::Choice(_) => ValueKind::Scalar,
        }
    }
}

fn item_path(path: &str) -> String {
    if path.is_empty() {
        "*".to_string()
    } else {
        format!("{path}.*")
    }
}

#[allow(clippy::too_many_arguments)]
fn push_output_path(
    outputs: &mut Vec<HelperFragmentOutputUse>,
    path: &str,
    relative_path: &YamlPath,
    kind: ValueKind,
    meta: Option<&HelperOutputMeta>,
    encoded_paths: &BTreeSet<String>,
    active_output_predicates: &BTreeSet<Predicate>,
    defaulted_paths: &BTreeSet<String>,
) {
    let base_meta = meta.cloned().unwrap_or_default();
    let mut meta = base_meta;
    meta.defaulted |= defaulted_paths.contains(path);
    let meta = meta.with_output_site_predicates(path, active_output_predicates);
    outputs.push(HelperFragmentOutputUse::with_encoding(
        path.to_string(),
        relative_path.clone(),
        kind,
        path_is_encoded(path, encoded_paths),
        meta,
    ));
}

fn path_is_encoded(path: &str, encoded_paths: &BTreeSet<String>) -> bool {
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
