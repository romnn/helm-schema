use std::collections::{BTreeMap, BTreeSet};

use crate::helper_meta::HelperOutputMeta;
use helm_schema_core::{Predicate, ValuesPath};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[expect(
    clippy::large_enum_variant,
    reason = "boxing common abstract values would add allocation and pointer chasing in the evaluator hot path"
)]
pub(crate) enum AbstractValue {
    Top,
    Unknown,
    ValuesPath(ValuesPath),
    /// Input identity whose runtime value came back through JSON decoding.
    JsonDecodedPath(ValuesPath),
    /// Key produced by directly ranging a values-backed collection.
    RangeKey(ValuesPath),
    /// The list of keys of the values-backed collection at path (`keys m`).
    /// Ranging it binds the item to [`Self::RangeKey`], so plucking that
    /// key back out of the same collection is a member projection.
    KeysList(ValuesPath),
    OutputPath(ValuesPath, HelperOutputMeta),
    RootContext,
    StringSet(BTreeSet<String>),
    /// Boolean result computed from other values. Its influences are not
    /// the Boolean's raw runtime identity.
    DerivedBoolean(BTreeSet<ValuesPath>),
    Dict(BTreeMap<String, AbstractValue>),
    List(Vec<AbstractValue>),
    Overlay {
        entries: BTreeMap<String, AbstractValue>,
        fallback: Box<AbstractValue>,
    },
    Choice(BTreeSet<AbstractValue>),
    /// First-Helm-truthy selection among ordered candidates (a `default`
    /// chain: `A | default B | default C`). The runtime value is the first
    /// truthy candidate, or the LAST candidate verbatim when none is truthy
    /// (`default` never tests its fallback). Influence-wise it behaves like
    /// [`Self::Choice`] over the candidates; the ordering survives so a
    /// range over the selected value can claim per-candidate iterable
    /// domains (`truthy(A) ⇒ iterable(A)`, `¬truthy(A) ∧ truthy(B) ⇒
    /// iterable(B)`) instead of the self-truthy approximation that would
    /// falsely reject a truthy scalar beside a selected collection.
    FirstTruthy(Vec<AbstractValue>),
    /// Ordered Sprig `merge` of values-backed layers, highest precedence
    /// first: an earlier layer's key shadows the same key of every later
    /// layer. Influence-wise it behaves like [`Self::Choice`] over the
    /// layers; the ordering survives so sink projection can scope a later
    /// layer's member contracts to keys the earlier layers do not supply.
    MergedLayers(Vec<AbstractValue>),
    /// List produced by splitting derived text. The source paths are
    /// influences rather than list identities; literal indexing uses the
    /// separator to recover a bounded source-cardinality precondition.
    SplitList {
        source_paths: BTreeSet<ValuesPath>,
        separator: String,
        total_text_preimage: bool,
    },
    /// One separator-delimited segment of split derived text (`regexSplit
    /// ":" . -1 | last`). Like [`Self::SplitList`], the source paths are
    /// influences, but a typed sink can constrain the named segment.
    SplitSegment {
        source_paths: BTreeSet<ValuesPath>,
        separator: String,
        /// The LAST segment when true, the first otherwise.
        last: bool,
        total_text_preimage: bool,
    },
    /// Result of a call without a transfer function: the value itself is
    /// unknown (structural operations treat it like `Unknown`), but the
    /// `.Values` paths that flowed into the call are kept so output
    /// projection can still attribute the rendered text to its sources.
    /// Declared last so projected rows sort after structured alternatives.
    Widened(BTreeSet<ValuesPath>),
}

/// The raw-path prefix of a first-truthy selection chain, in selection
/// order (see [`AbstractValue::selection_chain_identity_paths`]). The
/// variant decides what the last path means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SelectionChainIdentities {
    /// Every candidate is a raw path: the last one is the terminal
    /// fallback, selected whenever it is reached.
    Complete { paths: Vec<ValuesPath> },
    /// A candidate that is not a raw path follows: the last path is
    /// selected by its own truthiness like every other prefix path.
    Prefix { paths: Vec<ValuesPath> },
}

impl AbstractValue {
    /// Number of leaf alternatives a structural consumer would copy while
    /// descending this value.
    ///
    /// The count deliberately ignores container depth: a single deeply
    /// nested path stays precise, while wide dictionaries and choices pay for
    /// every independently propagated leaf.
    pub(crate) fn structural_width(&self) -> usize {
        match self {
            Self::Dict(entries) => entries
                .values()
                .map(Self::structural_width)
                .sum::<usize>()
                .max(1),
            Self::List(items) | Self::FirstTruthy(items) | Self::MergedLayers(items) => items
                .iter()
                .map(Self::structural_width)
                .sum::<usize>()
                .max(1),
            Self::Overlay { entries, fallback } => entries
                .values()
                .map(Self::structural_width)
                .chain(std::iter::once(fallback.structural_width()))
                .sum::<usize>()
                .max(1),
            Self::Choice(choices) => choices
                .iter()
                .map(Self::structural_width)
                .sum::<usize>()
                .max(1),
            Self::StringSet(strings) => strings.len().max(1),
            Self::Top
            | Self::Unknown
            | Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::RangeKey(_)
            | Self::KeysList(_)
            | Self::OutputPath(_, _)
            | Self::RootContext
            | Self::DerivedBoolean(_)
            | Self::SplitList { .. }
            | Self::SplitSegment { .. }
            | Self::Widened(_) => 1,
        }
    }

    pub(crate) fn values_root() -> Self {
        Self::ValuesPath(ValuesPath::default())
    }

    pub(crate) fn paths(&self) -> BTreeSet<ValuesPath> {
        let mut paths = BTreeSet::new();
        self.collect_paths(&mut paths, true, false);
        paths
    }

    fn collect_paths(
        &self,
        out: &mut BTreeSet<ValuesPath>,
        descend_structures: bool,
        suppress_values_root: bool,
    ) {
        match self {
            Self::ValuesPath(path) => {
                if !suppress_values_root || path.segments().len() != 0 {
                    out.insert(path.clone());
                }
            }
            Self::JsonDecodedPath(path) => {
                if !suppress_values_root || path.segments().next().is_some() {
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
            Self::FirstTruthy(candidates) => {
                for value in candidates {
                    value.collect_paths(out, descend_structures, suppress_values_root);
                }
            }
            Self::MergedLayers(layers) => {
                for value in layers {
                    value.collect_paths(out, descend_structures, suppress_values_root);
                }
            }
            // The derived keys list influences output without carrying the
            // collection's member values.
            Self::KeysList(path) => {
                if !suppress_values_root {
                    out.insert(path.clone());
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
            }
            | Self::SplitSegment {
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
            Self::FirstTruthy(candidates) => candidates.iter().flat_map(Self::strings).collect(),
            _ => BTreeSet::new(),
        }
    }

    pub(crate) fn output_meta(&self) -> BTreeMap<ValuesPath, HelperOutputMeta> {
        let mut out = BTreeMap::new();
        self.collect_output_meta(&mut out);
        out
    }

    pub(crate) fn with_output_meta(
        self,
        meta_by_path: &BTreeMap<ValuesPath, HelperOutputMeta>,
    ) -> Self {
        match self {
            Self::ValuesPath(path) => match meta_by_path.get(&path) {
                Some(meta) => {
                    let mut meta = meta.clone();
                    meta.input_identity = true;
                    Self::OutputPath(path, meta)
                }
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
            Self::FirstTruthy(candidates) => Self::FirstTruthy(
                candidates
                    .into_iter()
                    .map(|value| value.with_output_meta(meta_by_path))
                    .collect(),
            ),
            other => other,
        }
    }

    fn collect_output_meta(&self, out: &mut BTreeMap<ValuesPath, HelperOutputMeta>) {
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
            Self::FirstTruthy(candidates) => {
                for value in candidates {
                    value.collect_output_meta(out);
                }
            }
            Self::MergedLayers(layers) => {
                for value in layers {
                    value.collect_output_meta(out);
                }
                // When every layer IS a path identity, each layer path's
                // binding meta carries its place in the merge: a local
                // bound to the merge renders the MERGED value, so the
                // per-path rows the binding produces must keep the layer
                // scoping (shadowed members stay open, scrubbed layers
                // null-relax) instead of typing each path unconditionally.
                // Nested merges flatten in precedence order — nesting is
                // associative, so an inner merge's layers slot into the
                // outer order where the inner merge stood (airflow's
                // per-set merge over the celery-merged workers base).
                let identities: Option<Vec<ValuesPath>> =
                    layers.iter().map(Self::merge_layer_identity).collect();
                if let Some(layer_paths) = identities
                    && layer_paths.len() > 1
                    && layer_paths
                        .iter()
                        .all(|path| path.segments().next().is_some())
                {
                    let transforms = layers
                        .iter()
                        .map(|layer| match layer {
                            Self::OutputPath(_, meta) if meta.nil_scrubbed => {
                                helm_schema_core::MergeLayerTransform::NilScrubbed
                            }
                            Self::OutputPath(_, meta) if meta.parsed_map => {
                                helm_schema_core::MergeLayerTransform::ParsedMap
                            }
                            _ => helm_schema_core::MergeLayerTransform::Identity,
                        })
                        .collect::<Vec<_>>();
                    let merge_layers = layer_paths
                        .iter()
                        .cloned()
                        .zip(transforms.iter().copied())
                        .map(|(path, transform)| helm_schema_core::MergeLayer { path, transform })
                        .collect::<Vec<_>>();
                    for (position, layer_path) in layer_paths.iter().enumerate() {
                        let entry = out.entry(layer_path.clone()).or_default();
                        entry.merge_layers = helm_schema_core::MergeLayersUse::new(
                            merge_layers.clone(),
                            position,
                            true,
                        );
                    }
                }
            }
            Self::Top
            | Self::Unknown
            | Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::RangeKey(_)
            | Self::KeysList(_)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::SplitList { .. }
            | Self::SplitSegment { .. }
            | Self::Widened(_) => {}
        }
    }

    pub(crate) fn fragment_range_item(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) => {
                let mut path = path.clone();
                path.push_each_member();
                Some(Self::ValuesPath(path))
            }
            Self::KeysList(path) => Some(Self::RangeKey(path.clone())),
            Self::JsonDecodedPath(path) => Some(Self::JsonDecodedPath(item_path(path))),
            Self::OutputPath(path, meta) if meta.is_input_identity() || meta.json_decoded => {
                Some(Self::OutputPath(item_path(path), meta.clone()))
            }
            Self::OutputPath(path, meta) => Some(Self::OutputPath(path.clone(), meta.clone())),
            Self::List(items) => Self::choice(items.clone()),
            Self::SplitList { .. } | Self::SplitSegment { .. } => Some(Self::Unknown),
            Self::Choice(choices) => Self::choice(
                choices
                    .iter()
                    .filter_map(Self::fragment_range_item)
                    .collect(),
            ),
            // The item is a member of the SELECTED candidate; the unordered
            // union over-approximates that soundly.
            Self::FirstTruthy(candidates) => Self::choice(
                candidates
                    .iter()
                    .filter_map(Self::fragment_range_item)
                    .collect(),
            ),
            // Ranging the merged map visits members of every layer, so the
            // item domain is the union regardless of per-key precedence.
            Self::MergedLayers(layers) => Self::choice(
                layers
                    .iter()
                    .filter_map(Self::fragment_range_item)
                    .collect(),
            ),
            // An overlay's range visits its literal entries AND the members
            // of what it overlays, so the item domain is that union — the
            // same rule the merge layers follow. Without it, ranging a
            // `set`-mutated local dict (traefik's `$services :=
            // .Values.service.additionalServices` plus a synthetic
            // "default" entry) loses every member identity and the members'
            // own consumers bind nothing.
            Self::Overlay { entries, fallback } => Self::choice(
                entries
                    .values()
                    .cloned()
                    .chain(fallback.fragment_range_item())
                    .collect(),
            ),
            Self::Top
            | Self::Unknown
            | Self::RangeKey(_)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::Dict(_)
            | Self::Widened(_) => None,
        }
    }

    pub(crate) fn choice(values: Vec<Self>) -> Option<Self> {
        Self::join_all(values)
    }

    pub(crate) fn widened(paths: BTreeSet<ValuesPath>) -> Option<Self> {
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
            // Dropping a candidate would misstate the selection order, so a
            // chain that loses one degrades to the unordered choice.
            Self::FirstTruthy(candidates) => {
                let mapped: Vec<Option<Self>> =
                    candidates.into_iter().map(Self::without_widened).collect();
                let intact = mapped.iter().all(Option::is_some);
                let kept: Vec<Self> = mapped.into_iter().flatten().collect();
                if intact {
                    Self::first_truthy(kept)
                } else {
                    Self::choice(kept)
                }
            }
            other => Some(other),
        }
    }

    pub(crate) fn mark_json_decoded(self) -> Self {
        match self {
            Self::ValuesPath(path) => Self::JsonDecodedPath(path),
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
            Self::FirstTruthy(candidates) => Self::FirstTruthy(
                candidates
                    .into_iter()
                    .map(Self::mark_json_decoded)
                    .collect(),
            ),
            Self::MergedLayers(layers) => {
                Self::merged_layers(layers.into_iter().map(Self::mark_json_decoded).collect())
                    .unwrap_or(Self::Unknown)
            }
            Self::Widened(paths) => {
                Self::choice(paths.into_iter().map(Self::JsonDecodedPath).collect())
                    .unwrap_or(Self::Unknown)
            }
            Self::JsonDecodedPath(_)
            | Self::Top
            | Self::Unknown
            | Self::RangeKey(_)
            | Self::KeysList(_)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::SplitList { .. }
            | Self::SplitSegment { .. } => self,
        }
    }

    pub(crate) fn json_roundtrip_identity(&self) -> Option<Self> {
        fn member(value: &AbstractValue) -> AbstractValue {
            value
                .json_roundtrip_identity()
                .unwrap_or(AbstractValue::Unknown)
        }
        match self {
            Self::ValuesPath(_) | Self::JsonDecodedPath(_) | Self::OutputPath(_, _) => {
                Some(self.clone().mark_json_decoded())
            }
            Self::StringSet(_) | Self::DerivedBoolean(_) => Some(self.clone()),
            Self::Dict(entries) => Some(Self::Dict(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), member(value)))
                    .collect(),
            )),
            Self::List(items) => Some(Self::List(items.iter().map(member).collect())),
            Self::Overlay { entries, fallback } => Some(Self::Overlay {
                entries: entries
                    .iter()
                    .map(|(key, value)| (key.clone(), member(value)))
                    .collect(),
                fallback: Box::new(member(fallback)),
            }),
            Self::Choice(choices) => {
                Self::choice(choices.iter().map(member).collect()).map(Self::mark_json_decoded)
            }
            Self::FirstTruthy(candidates) => {
                Self::choice(candidates.iter().map(member).collect()).map(Self::mark_json_decoded)
            }
            _ => self.values_root_structure().map(Self::mark_json_decoded),
        }
    }

    pub(crate) fn values_root_structure(&self) -> Option<Self> {
        match self {
            Self::ValuesPath(path) if path.segments().len() == 0 => Some(self.clone()),
            Self::JsonDecodedPath(path) if path.segments().next().is_none() => Some(self.clone()),
            Self::OutputPath(path, _) if path.segments().next().is_none() => Some(self.clone()),
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
            Self::FirstTruthy(candidates) => Self::choice(
                candidates
                    .iter()
                    .filter_map(Self::values_root_structure)
                    .collect(),
            ),
            Self::MergedLayers(layers) => Self::choice(
                layers
                    .iter()
                    .filter_map(Self::values_root_structure)
                    .collect(),
            ),
            Self::Top
            | Self::Unknown
            | Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::RangeKey(_)
            | Self::KeysList(_)
            | Self::OutputPath(_, _)
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_)
            | Self::List(_)
            | Self::SplitList { .. }
            | Self::SplitSegment { .. }
            | Self::Widened(_) => None,
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

    /// Normalizing [`Self::MergedLayers`] constructor: nested merges flatten
    /// in precedence order, no layers is nothing, and one layer is itself.
    pub(crate) fn merged_layers(layers: Vec<Self>) -> Option<Self> {
        fn append_flat(layers: Vec<AbstractValue>, flat: &mut Vec<AbstractValue>) {
            for layer in layers {
                match layer {
                    AbstractValue::MergedLayers(nested) => append_flat(nested, flat),
                    other => flat.push(other),
                }
            }
        }

        let mut flat = Vec::new();
        append_flat(layers, &mut flat);
        match flat.len() {
            0 => None,
            1 => flat.into_iter().next(),
            _ => Some(Self::MergedLayers(flat)),
        }
    }

    /// Normalizing [`Self::FirstTruthy`] constructor: nested chains flatten
    /// (`a | default b | default c` selects exactly like the flat chain),
    /// and a chain whose candidates are all equal IS that value.
    pub(crate) fn first_truthy(candidates: Vec<Self>) -> Option<Self> {
        let mut flat = Vec::new();
        for candidate in candidates {
            match candidate {
                Self::FirstTruthy(inner) => flat.extend(inner),
                other => flat.push(other),
            }
        }
        flat.dedup();
        match flat.len() {
            0 => None,
            1 => flat.into_iter().next(),
            _ => Some(Self::FirstTruthy(flat)),
        }
    }

    /// The ordered raw-path identities a selection chain starts with. The
    /// decode stops at the first candidate that is not a raw path (see
    /// [`Self::selection_identity`]) and says so through the variant: the
    /// prefix is still ordered — each path is selected by its own
    /// truthiness once every earlier path is falsy — but only a
    /// [`SelectionChainIdentities::Complete`] chain names its terminal
    /// fallback. A consumer that would hand the terminal's states to the
    /// last path must abstain on a prefix, because those states select the
    /// literal or unresolved tail instead.
    pub(crate) fn selection_chain_identity_paths(&self) -> Option<SelectionChainIdentities> {
        let Self::FirstTruthy(candidates) = self else {
            return None;
        };
        let mut paths = Vec::new();
        for candidate in candidates {
            match candidate.selection_identity() {
                Some(path) => paths.push(path.clone()),
                None => break,
            }
        }
        if paths.is_empty() {
            return None;
        }
        if paths.len() == candidates.len() {
            Some(SelectionChainIdentities::Complete { paths })
        } else {
            Some(SelectionChainIdentities::Prefix { paths })
        }
    }

    /// The raw path whose truthiness selects this value as a first-truthy
    /// candidate, when the value IS that path. An [`Self::OutputPath`]
    /// qualifies only when its metadata proves structural input identity;
    /// transformed outputs have different truthiness and cannot be named by
    /// the raw path.
    pub(crate) fn selection_identity(&self) -> Option<&ValuesPath> {
        match self {
            Self::ValuesPath(path) | Self::JsonDecodedPath(path) => Some(path),
            Self::OutputPath(path, meta) if meta.is_input_identity() => Some(path),
            _ => None,
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic operation together makes its state transitions easier to audit"
    )]
    pub(crate) fn apply_to_path(&self, rest: &[String]) -> Option<Self> {
        if rest.is_empty() {
            return Some(self.clone());
        }

        match self {
            Self::ValuesPath(prefix) => {
                let mut path = prefix.clone();
                for segment in rest {
                    path.push(segment);
                }
                Some(Self::ValuesPath(path))
            }
            Self::JsonDecodedPath(prefix) => {
                let mut path = prefix.clone();
                for segment in rest {
                    path.push(segment);
                }
                Some(Self::JsonDecodedPath(path))
            }
            Self::OutputPath(prefix, meta) if meta.json_decoded => {
                let mut path = prefix.clone();
                for segment in rest {
                    path.push(segment);
                }
                let mut meta = meta.clone();
                // A fallback for the decoded container does not supply any
                // of its selected descendants.
                meta.defaulted = false;
                Some(Self::OutputPath(path, meta))
            }
            Self::OutputPath(prefix, meta)
                if meta.is_input_member_identity()
                    && rest
                        .first()
                        .is_none_or(|member| !meta.omitted_keys.contains_key(member)) =>
            {
                let mut path = prefix.clone();
                for segment in rest {
                    path.push(segment);
                }
                let mut meta = meta.clone();
                meta.defaulted = false;
                // `omit` describes members of the selected container. Once
                // an unomitted member is selected, those names say nothing
                // about the descendant value.
                meta.omitted_keys.clear();
                Some(Self::OutputPath(path, meta))
            }
            Self::OutputPath(prefix, meta) if meta.parsed_map => {
                let mut path = prefix.clone();
                for segment in rest {
                    path.push(segment);
                }
                let mut meta = meta.clone();
                // Map-only YAML decoding preserves the selected member's
                // identity, but a fallback for the container does not
                // supply that member.
                meta.defaulted = false;
                Some(Self::OutputPath(path, meta))
            }
            Self::OutputPath(prefix, meta) if meta.is_identity_preserving_default() => {
                let mut path = prefix.clone();
                for segment in rest {
                    path.push(segment);
                }
                let mut meta = meta.clone();
                meta.defaulted = false;
                Some(Self::OutputPath(path, meta))
            }
            Self::OutputPath(prefix, meta) => Some(Self::OutputPath(prefix.clone(), meta.clone())),
            Self::RootContext => {
                if let Some((segment, remaining)) = rest.split_first()
                    && segment == "Values"
                {
                    let tail = resolve_root_values_methods(remaining)?;
                    if tail.is_empty() {
                        Some(Self::values_root())
                    } else {
                        Some(Self::ValuesPath(ValuesPath::from_segments(
                            tail.iter().cloned(),
                        )))
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
            | Self::SplitSegment { .. }
            | Self::RangeKey(_)
            | Self::KeysList(_)
            | Self::StringSet(_) => None,
            Self::Choice(choices) => {
                let mut out = Vec::new();
                for value in choices {
                    if let Some(bound) = value.apply_to_path(rest) {
                        out.push(bound);
                    }
                }
                Self::choice(out)
            }
            // The member of the SELECTED candidate: selection tests the
            // candidates' own truthiness, not the members', so the ordering
            // does not transfer and the projection degrades to the choice.
            Self::FirstTruthy(candidates) => {
                let mut out = Vec::new();
                for value in candidates {
                    if let Some(bound) = value.apply_to_path(rest) {
                        out.push(bound);
                    }
                }
                Self::choice(out)
            }
            // Member selection distributes over the layers WITH their
            // precedence: mergo recurses into nested maps with the same
            // override order, so `merged.key` is itself a layered merge of
            // the per-layer members. Collapsing to an unordered choice here
            // would erase the shadowing that makes a fully-overridden base
            // member unreachable (kyverno's `featuresOverride.logging`).
            // A layer whose member cannot be resolved drops only when the
            // layer's shape PROVES the member absent; an opaque layer stays
            // as an unknown member that may still shadow everything below
            // it (airflow's nil-filtered celery overwrite layer).
            Self::MergedLayers(layers) => {
                let mut out = Vec::new();
                for value in layers {
                    match value.apply_to_path(rest) {
                        Some(bound) => out.push(bound),
                        None if value.member_projection_is_exhaustive() => {}
                        None => out.push(Self::Unknown),
                    }
                }
                Self::merged_layers(out)
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

    /// Whether a failed [`Self::apply_to_path`] on this value proves the
    /// member absent.
    ///
    /// Structured values enumerate their members (checked recursively,
    /// since a projection can fail at any depth) and scalars have none, so
    /// a miss is a genuine absence; opaque values (unknown call results,
    /// derived text) may hold the member without the projection seeing it.
    fn member_projection_is_exhaustive(&self) -> bool {
        match self {
            // Path-backed values and `Top` never fail a projection, and
            // scalar shapes genuinely have no members. `RootContext` fails
            // only on non-`Values` heads, which never name a user value.
            Self::ValuesPath(_)
            | Self::JsonDecodedPath(_)
            | Self::OutputPath(_, _)
            | Self::Top
            | Self::RootContext
            | Self::StringSet(_)
            | Self::DerivedBoolean(_) => true,
            Self::Dict(entries) => entries.values().all(Self::member_projection_is_exhaustive),
            Self::List(items) => items.iter().all(Self::member_projection_is_exhaustive),
            Self::Overlay { entries, fallback } => {
                entries.values().all(Self::member_projection_is_exhaustive)
                    && fallback.member_projection_is_exhaustive()
            }
            Self::Choice(choices) => choices.iter().all(Self::member_projection_is_exhaustive),
            Self::FirstTruthy(candidates) => {
                candidates.iter().all(Self::member_projection_is_exhaustive)
            }
            Self::MergedLayers(layers) => layers.iter().all(Self::member_projection_is_exhaustive),
            Self::Unknown
            | Self::RangeKey(_)
            | Self::KeysList(_)
            | Self::SplitList { .. }
            | Self::SplitSegment { .. }
            | Self::Widened(_) => false,
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
                Self::FirstTruthy(candidates) => pending.extend(candidates),
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

    pub(crate) fn unique_path(&self) -> Option<ValuesPath> {
        let mut paths = self.paths().into_iter();
        let first = paths.next()?;
        if paths.next().is_none() {
            Some(first)
        } else {
            None
        }
    }

    /// The values path a merge layer's runtime value IS.
    ///
    /// Accepts a direct path identity, possibly alternated with pathless
    /// literal arms (velero's `.Values.podSecurityContext | default dict`
    /// off-state).
    ///
    /// A constructed container REFERENCING a path returns `None` — its keys
    /// are template-supplied (external-dns's `merge $defaultSelector
    /// .podAffinityTerm` selector built from `nameOverride`), so keying the
    /// merge shadow on the referenced path would scope sibling-layer members
    /// by the wrong value.
    pub(crate) fn merge_layer_identity(&self) -> Option<ValuesPath> {
        fn arms_are_identity_or_literal(value: &AbstractValue) -> bool {
            match value {
                AbstractValue::ValuesPath(_)
                | AbstractValue::JsonDecodedPath(_)
                | AbstractValue::OutputPath(_, _) => true,
                // A nested merge of identities keeps the lineage: airflow's
                // per-set worker context resolves `.Values.workers.x` to
                // `MergedLayers([overwrite, workers.x])`, which still IS the
                // `workers.x` value wherever the overwrite abstains.
                AbstractValue::Choice(choices) => choices.iter().all(arms_are_identity_or_literal),
                AbstractValue::FirstTruthy(candidates) => {
                    candidates.iter().all(arms_are_identity_or_literal)
                }
                AbstractValue::MergedLayers(layers) => {
                    layers.iter().all(arms_are_identity_or_literal)
                }
                other => other.paths().is_empty(),
            }
        }
        arms_are_identity_or_literal(self).then(|| self.unique_path())?
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
            Self::FirstTruthy(candidates) => Self::FirstTruthy(
                candidates
                    .into_iter()
                    .map(|candidate| candidate.omit_keys(keys))
                    .collect(),
            ),
            other => other,
        }
    }

    pub(crate) fn fragment_source_paths(&self) -> BTreeSet<ValuesPath> {
        let mut paths = BTreeSet::new();
        self.collect_paths(&mut paths, false, true);
        paths
    }

    pub(crate) fn fragment_rendered_paths(&self) -> BTreeSet<ValuesPath> {
        let mut paths = BTreeSet::new();
        self.collect_paths(&mut paths, true, true);
        paths
    }
}

fn item_path(path: &ValuesPath) -> ValuesPath {
    let mut path = path.clone();
    path.push_each_member();
    path
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
    match tail.split_first() {
        Some((method, rest)) if method == "AsMap" => Some(rest),
        Some((method, _))
            if matches!(method.as_str(), "YAML" | "Table" | "Encode" | "PathValue") =>
        {
            None
        }
        _ => Some(tail),
    }
}

#[cfg(test)]
#[path = "tests/abstract_value.rs"]
mod tests;
