use std::collections::{BTreeMap, BTreeSet};

use crate::fragment_binding::FragmentBinding;
use crate::helper_binding::HelperBinding;
use crate::helper_summary::HelperOutputMeta;
use crate::{ValueKind, YamlPath};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AbstractValue {
    Top,
    Unknown,
    ValuesPath(String),
    RootContext,
    OutputSet(BTreeMap<String, HelperOutputMeta>),
    StringSet(BTreeSet<String>),
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
            Self::Top | Self::Unknown | Self::RootContext | Self::StringSet(_) => BTreeSet::new(),
        }
    }

    pub(crate) fn shallow_paths(&self) -> BTreeSet<String> {
        match self {
            Self::ValuesPath(path) => [path.clone()].into_iter().collect(),
            Self::OutputSet(outputs) => outputs.keys().cloned().collect(),
            Self::PathSet(paths) => paths.clone(),
            Self::Overlay { entries, fallback } => entries
                .values()
                .flat_map(Self::shallow_paths)
                .chain(fallback.shallow_paths())
                .collect(),
            Self::Choice(choices) => choices.iter().flat_map(Self::shallow_paths).collect(),
            Self::Top
            | Self::Unknown
            | Self::RootContext
            | Self::StringSet(_)
            | Self::Dict(_)
            | Self::List(_) => BTreeSet::new(),
        }
    }

    pub(crate) fn strings(&self) -> BTreeSet<String> {
        match self {
            Self::StringSet(strings) => strings.clone(),
            Self::Choice(choices) => choices.iter().flat_map(Self::strings).collect(),
            _ => BTreeSet::new(),
        }
    }

    pub(crate) fn output_meta(&self) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = BTreeMap::new();
        self.collect_output_meta(&mut out);
        out
    }

    fn collect_output_meta(&self, out: &mut BTreeMap<String, HelperOutputMeta>) {
        match self {
            Self::ValuesPath(path) => {
                out.entry(path.clone()).or_default();
            }
            Self::PathSet(paths) => {
                for path in paths {
                    out.entry(path.clone()).or_default();
                }
            }
            Self::OutputSet(meta_by_path) => {
                for (path, meta) in meta_by_path {
                    out.entry(path.clone()).or_default().merge_ref(meta);
                }
            }
            Self::Dict(entries) => {
                for value in entries.values() {
                    value.collect_output_meta(out);
                }
            }
            Self::List(items) => {
                for value in items {
                    value.collect_output_meta(out);
                }
            }
            Self::Overlay { entries, fallback } => {
                for value in entries.values() {
                    value.collect_output_meta(out);
                }
                fallback.collect_output_meta(out);
            }
            Self::Choice(choices) => {
                for choice in choices {
                    choice.collect_output_meta(out);
                }
            }
            Self::Top | Self::Unknown | Self::RootContext | Self::StringSet(_) => {}
        }
    }

    pub(crate) fn helper_range_item(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => Some(Self::ValuesPath(item_path(path))),
            Self::PathSet(paths) => Some(Self::PathSet(
                paths.iter().map(|path| item_path(path)).collect(),
            )),
            Self::OutputSet(outputs) => Some(Self::OutputSet(outputs.clone())),
            Self::Dict(entries) => Self::choice(entries.values().cloned().collect()),
            Self::List(items) => Self::choice(items.clone()),
            Self::Overlay { entries, fallback } => {
                let mut choices: Vec<_> = entries.values().cloned().collect();
                if let Some(fallback_item) = fallback.helper_range_item() {
                    choices.push(fallback_item);
                }
                Self::choice(choices)
            }
            Self::Choice(choices) => {
                Self::choice(choices.iter().filter_map(Self::helper_range_item).collect())
            }
            Self::Top | Self::Unknown | Self::RootContext | Self::StringSet(_) => None,
        }
    }

    pub(crate) fn fragment_range_item(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => Some(Self::ValuesPath(item_path(path))),
            Self::PathSet(paths) => Some(Self::PathSet(
                paths.iter().map(|path| item_path(path)).collect(),
            )),
            Self::OutputSet(outputs) => Some(Self::OutputSet(outputs.clone())),
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_values_root(&self) -> bool {
        matches!(self, Self::ValuesPath(path) if path.is_empty())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn fragment_output_paths(paths: impl IntoIterator<Item = String>) -> Self {
        Self::OutputSet(
            paths
                .into_iter()
                .map(|path| (path, HelperOutputMeta::default()))
                .collect(),
        )
    }

    pub(crate) fn merge_fragment_bindings(bindings: Vec<Self>) -> Option<Self> {
        let mut map = BTreeMap::new();
        let mut non_dict_value_paths = BTreeSet::new();
        let mut non_dict_output_meta: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        let mut non_dict_strings = BTreeSet::new();

        let mut pending = bindings;
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
                Self::PathSet(paths) => {
                    non_dict_value_paths.extend(paths);
                }
                Self::OutputSet(outputs) => {
                    for (path, meta) in outputs {
                        non_dict_output_meta.entry(path).or_default().merge(meta);
                    }
                }
                Self::StringSet(strings) => {
                    non_dict_strings.extend(strings);
                }
                Self::RootContext | Self::Unknown | Self::Top | Self::List(_) => {}
            }
        }

        let mut fallback_choices = Vec::new();
        if !non_dict_value_paths.is_empty() {
            fallback_choices.push(Self::PathSet(non_dict_value_paths));
        }
        if !non_dict_output_meta.is_empty() {
            fallback_choices.push(Self::OutputSet(non_dict_output_meta));
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

    pub(crate) fn remove_fragment_paths(self, remove: &BTreeSet<String>) -> Option<Self> {
        if remove.is_empty() {
            return Some(self);
        }

        match self {
            Self::ValuesPath(path) if remove.contains(&path) => None,
            Self::ValuesPath(_)
            | Self::RootContext
            | Self::Unknown
            | Self::Top
            | Self::StringSet(_) => Some(self),
            Self::OutputSet(mut outputs) => {
                outputs.retain(|path, _| !remove.contains(path));
                if outputs.is_empty() {
                    None
                } else {
                    Some(Self::OutputSet(outputs))
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

    pub(crate) fn from_helper_binding(binding: &HelperBinding) -> Self {
        binding.clone()
    }

    pub(crate) fn from_fragment_binding(binding: &FragmentBinding) -> Self {
        binding.clone()
    }

    /// Project a fragment binding for rendered-output attribution.
    ///
    /// An empty values-root path means the whole values document is available as
    /// data, not
    /// that the output path is the whole values document. Output attribution
    /// therefore abstains instead of inventing a root source path.
    pub(crate) fn from_fragment_output_binding(binding: &FragmentBinding) -> Self {
        match binding {
            Self::ValuesPath(path) if path.is_empty() => Self::Unknown,
            Self::ValuesPath(path) => Self::ValuesPath(path.clone()),
            Self::RootContext => Self::RootContext,
            Self::Unknown => Self::Unknown,
            Self::Top => Self::Top,
            Self::OutputSet(outputs) => Self::OutputSet(outputs.clone()),
            Self::StringSet(strings) => Self::StringSet(strings.clone()),
            Self::PathSet(paths) => Self::PathSet(paths.clone()),
            Self::Dict(map) => Self::Dict(
                map.iter()
                    .map(|(key, value)| (key.clone(), Self::from_fragment_output_binding(value)))
                    .collect(),
            ),
            Self::List(items) => Self::List(
                items
                    .iter()
                    .map(Self::from_fragment_output_binding)
                    .collect(),
            ),
            Self::Overlay { entries, fallback } => Self::Overlay {
                entries: entries
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from_fragment_output_binding(value)))
                    .collect(),
                fallback: Box::new(Self::from_fragment_output_binding(fallback)),
            },
            Self::Choice(choices) => Self::Choice(
                choices
                    .iter()
                    .map(Self::from_fragment_output_binding)
                    .collect::<BTreeSet<_>>(),
            ),
        }
    }

    pub(crate) fn for_output_path(
        source_expr: String,
        relative_path: &YamlPath,
        meta: HelperOutputMeta,
    ) -> Self {
        let mut value = Self::OutputSet(BTreeMap::from([(source_expr, meta)]));
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
            | Self::RootContext
            | Self::OutputSet(_)
            | Self::StringSet(_)
            | Self::PathSet(_)
            | Self::Choice(_) => ValueKind::Scalar,
        }
    }

    pub(crate) fn to_helper_binding(&self) -> Option<HelperBinding> {
        Some(match self {
            Self::Top => Self::Unknown,
            other => other.clone(),
        })
    }

    pub(crate) fn to_fragment_binding(&self) -> Option<FragmentBinding> {
        Some(match self {
            Self::Top => Self::Unknown,
            other => other.clone(),
        })
    }
}

fn item_path(path: &str) -> String {
    if path.is_empty() {
        "*".to_string()
    } else {
        format!("{path}.*")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> AbstractValue {
        AbstractValue::ValuesPath(value.to_string())
    }

    fn string(value: &str) -> AbstractValue {
        AbstractValue::StringSet(BTreeSet::from([value.to_string()]))
    }

    fn paths(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn join(values: Vec<AbstractValue>) -> AbstractValue {
        AbstractValue::join_all(values).expect("join should produce a value")
    }

    #[test]
    fn join_is_idempotent() {
        let value = path("image.tag");

        assert_eq!(join(vec![value.clone(), value.clone()]), value);
    }

    #[test]
    fn join_is_commutative() {
        let left = path("image.repository");
        let right = string("nginx");

        assert_eq!(
            join(vec![left.clone(), right.clone()]),
            join(vec![right, left])
        );
    }

    #[test]
    fn join_is_associative() {
        let left = path("image.repository");
        let middle = string("nginx");
        let right = path("image.tag");

        let left_grouped = join(vec![
            join(vec![left.clone(), middle.clone()]),
            right.clone(),
        ]);
        let right_grouped = join(vec![left, join(vec![middle, right])]);

        assert_eq!(left_grouped, right_grouped);
    }

    #[test]
    fn top_absorbs_join() {
        assert_eq!(
            join(vec![path("image.tag"), AbstractValue::Top]),
            AbstractValue::Top
        );
    }

    #[test]
    fn compatibility_unknown_widens_joins_to_top() {
        assert_eq!(
            join(vec![path("image.tag"), AbstractValue::Unknown]),
            AbstractValue::Top
        );
    }

    #[test]
    fn top_inside_choice_absorbs_join() {
        let nested = AbstractValue::Choice(BTreeSet::from([AbstractValue::Top, path("name")]));

        assert_eq!(join(vec![path("image.tag"), nested]), AbstractValue::Top);
    }

    #[test]
    fn top_propagates_through_descent() {
        assert_eq!(
            AbstractValue::Top.apply_to_path(&["nested".to_string()]),
            Some(AbstractValue::Top)
        );
    }

    #[test]
    fn omit_keys_removes_known_map_entries_but_preserves_values_root() {
        let value = AbstractValue::Overlay {
            entries: BTreeMap::from([
                ("enabled".to_string(), path("probe.enabled")),
                ("timeoutSeconds".to_string(), path("probe.timeoutSeconds")),
            ]),
            fallback: Box::new(path("probe")),
        };

        assert_eq!(
            value.omit_keys(&BTreeSet::from(["enabled".to_string()])),
            AbstractValue::Overlay {
                entries: BTreeMap::from([(
                    "timeoutSeconds".to_string(),
                    path("probe.timeoutSeconds")
                )]),
                fallback: Box::new(path("probe")),
            }
        );
    }

    #[test]
    fn shallow_paths_do_not_descend_structured_maps() {
        let value = AbstractValue::Dict(BTreeMap::from([(
            "metadata".to_string(),
            AbstractValue::ValuesPath("podLabels".to_string()),
        )]));

        assert_eq!(value.shallow_paths(), BTreeSet::new());
        assert_eq!(value.paths(), paths(&["podLabels"]));
    }

    #[test]
    fn helper_and_fragment_range_items_keep_distinct_structural_policy() {
        let value = AbstractValue::Dict(BTreeMap::from([(
            "name".to_string(),
            AbstractValue::ValuesPath("containers.name".to_string()),
        )]));

        assert_eq!(value.fragment_range_item(), None);
        assert_eq!(
            value.helper_range_item(),
            Some(AbstractValue::ValuesPath("containers.name".to_string()))
        );
    }

    #[test]
    fn output_meta_preserves_values_paths_and_output_set_metadata() {
        let meta = HelperOutputMeta {
            predicates: BTreeSet::new(),
            defaulted: true,
            provenance: Vec::new(),
        };
        let value = AbstractValue::Overlay {
            entries: BTreeMap::from([("name".to_string(), path("serviceAccount.name"))]),
            fallback: Box::new(AbstractValue::OutputSet(BTreeMap::from([(
                "global.nameOverride".to_string(),
                meta.clone(),
            )]))),
        };

        assert_eq!(
            value.output_meta(),
            BTreeMap::from([
                (
                    "serviceAccount.name".to_string(),
                    HelperOutputMeta::default()
                ),
                ("global.nameOverride".to_string(), meta),
            ])
        );
    }
}
