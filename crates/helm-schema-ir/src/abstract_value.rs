use std::collections::{BTreeMap, BTreeSet};

use crate::helper_meta::HelperOutputMeta;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AbstractValue {
    Top,
    Unknown,
    ValuesPath(String),
    /// Input identity whose runtime value came back through JSON decoding.
    JsonDecodedPath(String),
    /// Key produced by directly ranging a values-backed collection.
    RangeKey(String),
    OutputPath(String, HelperOutputMeta),
    RootContext,
    StringSet(BTreeSet<String>),
    /// Boolean result computed from other values. Its influences are not
    /// the Boolean's raw runtime identity.
    DerivedBoolean(BTreeSet<String>),
    Dict(BTreeMap<String, AbstractValue>),
    List(Vec<AbstractValue>),
    Overlay {
        entries: BTreeMap<String, AbstractValue>,
        fallback: Box<AbstractValue>,
    },
    Choice(BTreeSet<AbstractValue>),
    /// List produced by splitting derived text. The source paths are
    /// influences rather than list identities; literal indexing uses the
    /// separator to recover a bounded source-cardinality precondition.
    SplitList {
        source_paths: BTreeSet<String>,
        separator: String,
        total_text_preimage: bool,
    },
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

    pub(crate) fn range_key_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        self.collect_range_key_paths(&mut paths);
        paths
    }

    fn collect_range_key_paths(&self, out: &mut BTreeSet<String>) {
        match self {
            Self::RangeKey(path) => {
                out.insert(path.clone());
            }
            Self::Dict(map) => {
                for value in map.values() {
                    value.collect_range_key_paths(out);
                }
            }
            Self::List(items) => {
                for value in items {
                    value.collect_range_key_paths(out);
                }
            }
            Self::Overlay { entries, fallback } => entries
                .values()
                .chain(std::iter::once(fallback.as_ref()))
                .for_each(|value| value.collect_range_key_paths(out)),
            Self::Choice(choices) => {
                for value in choices {
                    value.collect_range_key_paths(out);
                }
            }
            Self::Top
            | Self::Unknown
            | Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::OutputPath(_, _)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::SplitList { .. }
            | Self::Widened(_) => {}
        }
    }

    fn collect_paths(
        &self,
        out: &mut BTreeSet<String>,
        descend_structures: bool,
        suppress_values_root: bool,
    ) {
        match self {
            Self::ValuesPath(path) | Self::JsonDecodedPath(path) => {
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
            Self::Widened(paths)
            | Self::DerivedBoolean(paths)
            | Self::SplitList {
                source_paths: paths,
                ..
            } => {
                if !suppress_values_root {
                    out.extend(paths.iter().cloned());
                }
            }
            Self::Top
            | Self::Unknown
            | Self::RangeKey(_)
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

    pub(crate) fn output_meta(&self) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = BTreeMap::new();
        self.collect_output_meta(&mut out);
        out
    }

    pub(crate) fn with_output_meta(
        self,
        meta_by_path: &BTreeMap<String, HelperOutputMeta>,
    ) -> Self {
        match self {
            Self::ValuesPath(path) => match meta_by_path.get(&path) {
                Some(meta) => Self::OutputPath(path, meta.clone()),
                None => Self::ValuesPath(path),
            },
            Self::JsonDecodedPath(path) => match meta_by_path.get(&path) {
                Some(meta) => {
                    let mut meta = meta.clone();
                    meta.json_decoded = true;
                    Self::OutputPath(path, meta)
                }
                None => Self::JsonDecodedPath(path),
            },
            Self::OutputPath(path, mut meta) => {
                if let Some(extra) = meta_by_path.get(&path) {
                    meta.merge(extra);
                }
                Self::OutputPath(path, meta)
            }
            Self::Dict(entries) => Self::Dict(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, value.with_output_meta(meta_by_path)))
                    .collect(),
            ),
            Self::List(items) => Self::List(
                items
                    .into_iter()
                    .map(|value| value.with_output_meta(meta_by_path))
                    .collect(),
            ),
            Self::Overlay { entries, fallback } => Self::Overlay {
                entries: entries
                    .into_iter()
                    .map(|(key, value)| (key, value.with_output_meta(meta_by_path)))
                    .collect(),
                fallback: Box::new(fallback.with_output_meta(meta_by_path)),
            },
            Self::Choice(choices) => Self::Choice(
                choices
                    .into_iter()
                    .map(|value| value.with_output_meta(meta_by_path))
                    .collect(),
            ),
            other => other,
        }
    }

    fn collect_output_meta(&self, out: &mut BTreeMap<String, HelperOutputMeta>) {
        match self {
            Self::OutputPath(path, meta) => out.entry(path.clone()).or_default().merge(meta),
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
                for value in choices {
                    value.collect_output_meta(out);
                }
            }
            Self::Top
            | Self::Unknown
            | Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::RangeKey(_)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::SplitList { .. }
            | Self::Widened(_) => {}
        }
    }

    pub(crate) fn require_rendered_source_presence(self) -> Self {
        match self {
            Self::ValuesPath(path) | Self::JsonDecodedPath(path) => {
                let mut meta = HelperOutputMeta::default();
                meta.conjoin_branches(&BTreeSet::from([helm_schema_core::Predicate::from(
                    helm_schema_core::Guard::Absent { path: path.clone() },
                )
                .negated()]));
                Self::OutputPath(path, meta)
            }
            Self::OutputPath(path, mut meta) => {
                meta.conjoin_branches(&BTreeSet::from([helm_schema_core::Predicate::from(
                    helm_schema_core::Guard::Absent { path: path.clone() },
                )
                .negated()]));
                Self::OutputPath(path, meta)
            }
            Self::Dict(entries) => Self::Dict(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, value.require_rendered_source_presence()))
                    .collect(),
            ),
            Self::List(items) => Self::List(
                items
                    .into_iter()
                    .map(Self::require_rendered_source_presence)
                    .collect(),
            ),
            Self::Overlay { entries, fallback } => Self::Overlay {
                entries: entries
                    .into_iter()
                    .map(|(key, value)| (key, value.require_rendered_source_presence()))
                    .collect(),
                fallback: Box::new(fallback.require_rendered_source_presence()),
            },
            Self::Choice(choices) => Self::Choice(
                choices
                    .into_iter()
                    .map(Self::require_rendered_source_presence)
                    .collect(),
            ),
            other => other,
        }
    }

    pub(crate) fn fragment_range_item(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => Some(Self::ValuesPath(item_path(path))),
            Self::JsonDecodedPath(path) => Some(Self::JsonDecodedPath(item_path(path))),
            Self::OutputPath(path, meta) if meta.json_decoded => {
                Some(Self::OutputPath(item_path(path), meta.clone()))
            }
            Self::OutputPath(path, meta) => Some(Self::OutputPath(path.clone(), meta.clone())),
            Self::List(items) => Self::choice(items.clone()),
            Self::SplitList { .. } => Some(Self::Unknown),
            Self::Choice(choices) => Self::choice(
                choices
                    .iter()
                    .filter_map(Self::fragment_range_item)
                    .collect(),
            ),
            Self::Top
            | Self::Unknown
            | Self::RangeKey(_)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::Dict(_)
            | Self::Overlay { .. }
            | Self::Widened(_) => None,
        }
    }

    pub(crate) fn definitely_nonempty_iterable(&self) -> bool {
        match self {
            Self::Dict(entries) => !entries.is_empty(),
            Self::List(items) => !items.is_empty(),
            Self::Overlay { entries, .. } => !entries.is_empty(),
            Self::SplitList { .. } => true,
            Self::Choice(choices) => {
                !choices.is_empty() && choices.iter().all(Self::definitely_nonempty_iterable)
            }
            _ => false,
        }
    }

    pub(crate) fn static_truthiness(&self) -> Option<bool> {
        match self {
            Self::StringSet(strings) => {
                let all_empty = strings.iter().all(String::is_empty);
                let all_nonempty = strings.iter().all(|value| !value.is_empty());
                match (all_empty, all_nonempty) {
                    (true, false) => Some(false),
                    (false, true) => Some(true),
                    _ => None,
                }
            }
            Self::Dict(entries) => Some(!entries.is_empty()),
            Self::List(items) => Some(!items.is_empty()),
            Self::Overlay { entries, fallback } => (!entries.is_empty())
                .then_some(true)
                .or_else(|| fallback.static_truthiness()),
            Self::RootContext => Some(true),
            Self::Choice(choices) => {
                let mut truthiness = choices.iter().map(Self::static_truthiness);
                let first = truthiness.next()??;
                truthiness
                    .all(|candidate| candidate == Some(first))
                    .then_some(first)
            }
            Self::Top
            | Self::Unknown
            | Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::RangeKey(_)
            | Self::OutputPath(_, _)
            | Self::DerivedBoolean(_)
            | Self::SplitList { .. }
            | Self::Widened(_) => None,
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

    pub(crate) fn mark_json_decoded(self) -> Self {
        match self {
            Self::ValuesPath(path) => Self::JsonDecodedPath(path),
            Self::JsonDecodedPath(_) => self,
            Self::OutputPath(path, mut meta) => {
                meta.json_decoded = true;
                Self::OutputPath(path, meta)
            }
            Self::Dict(entries) => Self::Dict(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, value.mark_json_decoded()))
                    .collect(),
            ),
            Self::List(items) => {
                Self::List(items.into_iter().map(Self::mark_json_decoded).collect())
            }
            Self::Overlay { entries, fallback } => Self::Overlay {
                entries: entries
                    .into_iter()
                    .map(|(key, value)| (key, value.mark_json_decoded()))
                    .collect(),
                fallback: Box::new(fallback.mark_json_decoded()),
            },
            Self::Choice(choices) => {
                Self::Choice(choices.into_iter().map(Self::mark_json_decoded).collect())
            }
            Self::Widened(paths) => {
                Self::choice(paths.into_iter().map(Self::JsonDecodedPath).collect())
                    .unwrap_or(Self::Unknown)
            }
            Self::Top
            | Self::Unknown
            | Self::RangeKey(_)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::SplitList { .. } => self,
        }
    }

    /// Keep only identities that remain unambiguous across a JSON roundtrip.
    /// Direct values paths retain their identity. A structured helper value
    /// may retain the `.Values` root together with the literal wrapper keys
    /// needed to select it, but mutable sibling structure stays opaque.
    pub(crate) fn json_roundtrip_identity(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(_) | Self::JsonDecodedPath(_) | Self::OutputPath(_, _) => {
                Some(self.clone().mark_json_decoded())
            }
            _ => self.values_root_structure().map(Self::mark_json_decoded),
        }
    }

    pub(crate) fn values_root_structure(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) | Self::JsonDecodedPath(path) if path.is_empty() => {
                Some(self.clone())
            }
            Self::OutputPath(path, _) if path.is_empty() => Some(self.clone()),
            Self::Dict(entries) => {
                let entries = entries
                    .iter()
                    .filter_map(|(key, value)| {
                        value
                            .values_root_structure()
                            .map(|value| (key.clone(), value))
                    })
                    .collect::<BTreeMap<_, _>>();
                (!entries.is_empty()).then_some(Self::Dict(entries))
            }
            Self::Overlay { entries, fallback } => {
                if let Some(fallback) = fallback.values_root_structure() {
                    return Some(fallback);
                }
                let entries = entries
                    .iter()
                    .filter_map(|(key, value)| {
                        value
                            .values_root_structure()
                            .map(|value| (key.clone(), value))
                    })
                    .collect::<BTreeMap<_, _>>();
                (!entries.is_empty()).then_some(Self::Dict(entries))
            }
            Self::Choice(choices) => Self::choice(
                choices
                    .iter()
                    .filter_map(Self::values_root_structure)
                    .collect(),
            ),
            Self::Top
            | Self::Unknown
            | Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::RangeKey(_)
            | Self::OutputPath(_, _)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::List(_)
            | Self::SplitList { .. }
            | Self::Widened(_) => None,
        }
    }

    pub(crate) fn unique_json_decoded_path(&self) -> Option<String> {
        let path = match self {
            Self::JsonDecodedPath(path) => path,
            Self::OutputPath(path, meta) if meta.json_decoded => path,
            Self::Choice(choices)
                if !choices.is_empty()
                    && choices
                        .iter()
                        .all(|choice| choice.unique_json_decoded_path().is_some()) =>
            {
                let mut paths = choices.iter().filter_map(Self::unique_json_decoded_path);
                let first = paths.next()?;
                return paths.all(|path| path == first).then_some(first);
            }
            _ => return None,
        };
        Some(path.clone())
    }

    pub(crate) fn is_definitely_json_serialized(&self) -> bool {
        match self {
            Self::OutputPath(_, meta) => meta.json_serialized,
            Self::Dict(entries) => {
                !entries.is_empty() && entries.values().all(Self::is_definitely_json_serialized)
            }
            Self::List(items) => {
                !items.is_empty() && items.iter().all(Self::is_definitely_json_serialized)
            }
            Self::Overlay { entries, fallback } => {
                !entries.is_empty()
                    && entries.values().all(Self::is_definitely_json_serialized)
                    && fallback.is_definitely_json_serialized()
            }
            Self::Choice(choices) => {
                !choices.is_empty() && choices.iter().all(Self::is_definitely_json_serialized)
            }
            Self::Top
            | Self::Unknown
            | Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::RangeKey(_)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::SplitList { .. }
            | Self::Widened(_) => false,
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
                let mut segments = helm_schema_core::split_value_path(prefix);
                segments.extend(rest.iter().cloned());
                Some(Self::ValuesPath(helm_schema_core::join_value_path(
                    segments,
                )))
            }
            Self::JsonDecodedPath(prefix) => {
                let mut segments = helm_schema_core::split_value_path(prefix);
                segments.extend(rest.iter().cloned());
                Some(Self::JsonDecodedPath(helm_schema_core::join_value_path(
                    segments,
                )))
            }
            Self::OutputPath(prefix, meta) if meta.json_decoded => {
                let mut segments = helm_schema_core::split_value_path(prefix);
                segments.extend(rest.iter().cloned());
                Some(Self::OutputPath(
                    helm_schema_core::join_value_path(segments),
                    meta.clone(),
                ))
            }
            Self::OutputPath(prefix, meta) => Some(Self::OutputPath(prefix.clone(), meta.clone())),
            Self::RootContext => {
                if rest.first().is_some_and(|segment| segment == "Values") {
                    let tail = resolve_root_values_methods(&rest[1..])?;
                    if tail.is_empty() {
                        Some(Self::values_root())
                    } else {
                        Some(Self::ValuesPath(helm_schema_core::join_value_path(tail)))
                    }
                } else {
                    None
                }
            }
            Self::Top => Some(Self::Top),
            // Selecting into an unknown call result severs the influence:
            // the selected member is not derived from the recorded paths in
            // any way the projection could still attribute.
            Self::Unknown
            | Self::Widened(_)
            | Self::DerivedBoolean(_)
            | Self::SplitList { .. }
            | Self::RangeKey(_) => None,
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
            Self::JsonDecodedPath(path) if remove.contains(&path) => None,
            Self::OutputPath(path, _) if remove.contains(&path) => None,
            Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::RangeKey(_)
            | Self::OutputPath(_, _)
            | Self::RootContext
            | Self::Unknown
            | Self::Top
            | Self::StringSet(_) => Some(self),
            Self::DerivedBoolean(paths) => Some(Self::DerivedBoolean(
                paths.difference(remove).cloned().collect(),
            )),
            Self::Widened(paths) => Self::widened(paths.difference(remove).cloned().collect()),
            Self::SplitList {
                source_paths,
                separator,
                total_text_preimage,
            } => {
                let source_paths: BTreeSet<String> =
                    source_paths.difference(remove).cloned().collect();
                (!source_paths.is_empty()).then_some(Self::SplitList {
                    source_paths,
                    separator,
                    total_text_preimage,
                })
            }
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
            Self::JsonDecodedPath(path) => Some(Self::JsonDecodedPath(path.clone())),
            Self::OutputPath(path, meta) => Some(Self::OutputPath(path.clone(), meta.clone())),
            Self::RootContext => Some(Self::RootContext),
            Self::Top
            | Self::Unknown
            | Self::RangeKey(_)
            | Self::Dict(_)
            | Self::List(_)
            | Self::Overlay { .. }
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::Choice(_)
            | Self::SplitList { .. }
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
    helm_schema_core::append_value_path(path, "*")
}

/// Go-template method resolution on the typed root values object.
///
/// Helm's `.Values` is `chartutil.Values`, a named map type whose exported
/// methods shadow same-named map keys during selector resolution. A leading
/// `AsMap` calls the niladic method that returns the receiver map itself
/// (an empty map for nil/empty values), so the remaining segments continue
/// from the root. The other exposed methods (`YAML`, `Table`, `Encode`,
/// `PathValue`) take arguments or produce derived text, so selecting
/// through them never names a user value and resolution abstains instead
/// of fabricating a path segment. Only the ROOT receiver carries the type;
/// nested values are plain maps, so deeper same-named segments stay
/// ordinary keys.
pub(crate) fn resolve_root_values_methods(tail: &[String]) -> Option<&[String]> {
    match tail.first().map(String::as_str) {
        Some("AsMap") => Some(&tail[1..]),
        Some("YAML" | "Table" | "Encode" | "PathValue") => None,
        _ => Some(tail),
    }
}

pub(crate) fn path_is_encoded(path: &str, encoded_paths: &BTreeSet<String>) -> bool {
    encoded_paths.iter().any(|encoded_path| {
        path == encoded_path || helm_schema_core::values_path_is_descendant(path, encoded_path)
    })
}

#[cfg(test)]
#[path = "tests/abstract_value.rs"]
mod tests;
