use std::collections::{BTreeMap, BTreeSet};

use crate::binding::{FragmentBinding, HelperBinding};
use crate::helper_analysis::HelperOutputMeta;

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

    pub(crate) fn strings(&self) -> BTreeSet<String> {
        match self {
            Self::StringSet(strings) => strings.clone(),
            Self::Choice(choices) => choices.iter().flat_map(Self::strings).collect(),
            _ => BTreeSet::new(),
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

    pub(crate) fn from_helper_binding(binding: &HelperBinding) -> Self {
        match binding {
            HelperBinding::ValuesPath(path) => Self::ValuesPath(path.clone()),
            HelperBinding::RootContext => Self::RootContext,
            HelperBinding::Unknown => Self::Unknown,
            HelperBinding::OutputSet(outputs) => Self::OutputSet(outputs.clone()),
            HelperBinding::StringSet(strings) => Self::StringSet(strings.clone()),
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

    pub(crate) fn from_fragment_binding(binding: &FragmentBinding) -> Self {
        match binding {
            FragmentBinding::ValuesPath(path) => Self::ValuesPath(path.clone()),
            FragmentBinding::ValuesRoot => Self::ValuesPath(String::new()),
            FragmentBinding::RootContext => Self::RootContext,
            FragmentBinding::Unknown => Self::Unknown,
            FragmentBinding::OutputSet(paths) => Self::OutputSet(
                paths
                    .iter()
                    .map(|path| (path.clone(), HelperOutputMeta::default()))
                    .collect(),
            ),
            FragmentBinding::StringSet(strings) => Self::StringSet(strings.clone()),
            FragmentBinding::PathSet(paths) => Self::PathSet(paths.clone()),
            FragmentBinding::Dict(map) => Self::Dict(
                map.iter()
                    .map(|(key, value)| (key.clone(), Self::from_fragment_binding(value)))
                    .collect(),
            ),
            FragmentBinding::List(items) => {
                Self::List(items.iter().map(Self::from_fragment_binding).collect())
            }
            FragmentBinding::Overlay { entries, fallback } => Self::Overlay {
                entries: entries
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from_fragment_binding(value)))
                    .collect(),
                fallback: Box::new(Self::from_fragment_binding(fallback)),
            },
            FragmentBinding::Choice(choices) => Self::Choice(
                choices
                    .iter()
                    .map(Self::from_fragment_binding)
                    .collect::<BTreeSet<_>>(),
            ),
        }
    }

    pub(crate) fn to_helper_binding(&self) -> Option<HelperBinding> {
        match self {
            Self::ValuesPath(path) => Some(HelperBinding::ValuesPath(path.clone())),
            Self::Top => Some(HelperBinding::Unknown),
            Self::RootContext => Some(HelperBinding::RootContext),
            Self::Unknown => Some(HelperBinding::Unknown),
            Self::StringSet(strings) => Some(HelperBinding::StringSet(strings.clone())),
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

    pub(crate) fn to_fragment_binding(&self) -> Option<FragmentBinding> {
        match self {
            Self::ValuesPath(path) if path.is_empty() => Some(FragmentBinding::ValuesRoot),
            Self::ValuesPath(path) => Some(FragmentBinding::ValuesPath(path.clone())),
            Self::Top => Some(FragmentBinding::Unknown),
            Self::RootContext => Some(FragmentBinding::RootContext),
            Self::Unknown => Some(FragmentBinding::Unknown),
            Self::OutputSet(outputs) => Some(FragmentBinding::OutputSet(
                outputs.keys().cloned().collect(),
            )),
            Self::StringSet(strings) => Some(FragmentBinding::StringSet(strings.clone())),
            Self::PathSet(paths) => Some(FragmentBinding::PathSet(paths.clone())),
            Self::Dict(map) => Some(FragmentBinding::Dict(
                map.iter()
                    .map(|(key, value)| Some((key.clone(), value.to_fragment_binding()?)))
                    .collect::<Option<BTreeMap<_, _>>>()?,
            )),
            Self::List(items) => Some(FragmentBinding::List(
                items
                    .iter()
                    .map(Self::to_fragment_binding)
                    .collect::<Option<Vec<_>>>()?,
            )),
            Self::Overlay { entries, fallback } => Some(FragmentBinding::Overlay {
                entries: entries
                    .iter()
                    .map(|(key, value)| Some((key.clone(), value.to_fragment_binding()?)))
                    .collect::<Option<BTreeMap<_, _>>>()?,
                fallback: Box::new(fallback.to_fragment_binding()?),
            }),
            Self::Choice(choices) => FragmentBinding::choice(
                choices
                    .iter()
                    .filter_map(Self::to_fragment_binding)
                    .collect(),
            ),
        }
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
}
