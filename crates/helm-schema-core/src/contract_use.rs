use serde::ser::SerializeStruct as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ContractProvenance, Guard, GuardDnf, ResourceRef, ValueKind, ValuesPath, YamlPath};

/// The rendered text is ONE SEGMENT of the source string split by a literal
/// separator (`regexSplit ":" . -1 | last` extracting a port suffix): the
/// sink schema constrains that segment, never the whole raw value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SplitSegmentUse {
    /// Literal delimiter used to split the source string.
    pub separator: String,
    /// The LAST segment when true, the first otherwise.
    pub last: bool,
}

/// A structural transform applied to one ordered merge layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MergeLayerTransform {
    /// The layer reaches the merge unchanged.
    Identity,
    /// Nil members are recursively removed before the layer reaches the merge.
    NilScrubbed,
    /// Helm's map-only YAML decoder discards non-mapping source shapes.
    ParsedMap,
}

/// One ordered merge input paired with the transform applied before merging.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MergeLayer {
    /// Values path supplying this layer.
    pub path: ValuesPath,
    /// Structural transform applied before the merge reads the layer.
    pub transform: MergeLayerTransform,
}

/// The value is one layer of an ordered Sprig `merge`: a key of an earlier
/// layer shadows the same key of every later layer at the rendered sink, so
/// a later layer's member reaches the sink only where every earlier layer
/// lacks that member.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MergeLayersUse {
    layers: Vec<MergeLayer>,
    position: usize,
    own_transform: MergeLayerTransform,
    /// Whether the layer facts came from a local binding's metadata rather
    /// than the render site's own layered value.
    ///
    /// Identity-only binding merges keep their ordinary branch routing
    /// because sibling dispatch arms may contribute other input shapes.
    /// Structurally transformed bindings retain layered routing so each
    /// transform's selection semantics remain visible at emission.
    via_binding: bool,
}

impl MergeLayersUse {
    /// Creates a layered use when `position` selects an entry in `layers`.
    #[must_use]
    pub fn new(layers: Vec<MergeLayer>, position: usize, via_binding: bool) -> Option<Self> {
        let own_transform = layers
            .iter()
            .enumerate()
            .find_map(|(index, layer)| (index == position).then_some(layer.transform))?;
        Some(Self {
            layers,
            position,
            own_transform,
            via_binding,
        })
    }

    /// Returns every layer in precedence order.
    #[must_use]
    pub fn layers(&self) -> &[MergeLayer] {
        &self.layers
    }

    /// Returns this use's checked index in [`Self::layers`].
    #[must_use]
    pub fn position(&self) -> usize {
        self.position
    }

    /// Reports whether the own layer has `path`.
    #[must_use]
    pub fn own_path_is(&self, path: &ValuesPath) -> bool {
        self.layers
            .iter()
            .enumerate()
            .any(|(index, layer)| index == self.position && &layer.path == path)
    }

    /// Returns the higher-precedence layers whose keys shadow this layer's.
    #[must_use]
    pub fn shadowed_by(&self) -> impl ExactSizeIterator<Item = &MergeLayer> {
        self.layers.iter().take(self.position)
    }

    /// Returns the transform applied to this use's layer.
    #[must_use]
    pub fn own_transform(&self) -> MergeLayerTransform {
        self.own_transform
    }

    /// Reports whether any layer is structurally transformed.
    #[must_use]
    pub fn has_transformed_layer(&self) -> bool {
        self.layers
            .iter()
            .any(|layer| layer.transform != MergeLayerTransform::Identity)
    }

    /// Reports whether these facts crossed a local binding boundary.
    #[must_use]
    pub fn via_binding(&self) -> bool {
        self.via_binding
    }

    /// Marks these layer facts as crossing a local binding boundary.
    #[must_use]
    pub fn into_via_binding(mut self) -> Self {
        self.via_binding = true;
        self
    }
}

impl Serialize for MergeLayersUse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let paths = self
            .layers
            .iter()
            .map(|layer| &layer.path)
            .collect::<Vec<_>>();
        let transforms = self
            .layers
            .iter()
            .map(|layer| layer.transform)
            .collect::<Vec<_>>();
        let mut state = serializer.serialize_struct("MergeLayersUse", 4)?;
        state.serialize_field("layers", &paths)?;
        state.serialize_field("position", &self.position)?;
        state.serialize_field("transforms", &transforms)?;
        state.serialize_field("via_binding", &self.via_binding)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for MergeLayersUse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireMergeLayersUse {
            layers: Vec<ValuesPath>,
            position: usize,
            transforms: Vec<MergeLayerTransform>,
            via_binding: bool,
        }

        let wire = WireMergeLayersUse::deserialize(deserializer)?;
        if wire.layers.len() != wire.transforms.len() {
            return Err(serde::de::Error::custom(
                "merge layer paths and transforms must have equal lengths",
            ));
        }
        let layers = wire
            .layers
            .into_iter()
            .zip(wire.transforms)
            .map(|(path, transform)| MergeLayer { path, transform })
            .collect();
        Self::new(layers, wire.position, wire.via_binding)
            .ok_or_else(|| serde::de::Error::custom("merge layer position is out of bounds"))
    }
}

/// A contract claim for one observed values path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractUse {
    /// Canonical values path or expression that supplied the rendered value.
    pub source_expr: ValuesPath,
    /// Structural path of the value in the rendered YAML document.
    pub path: YamlPath,
    /// How the value contributes to the rendered YAML node.
    pub kind: ValueKind,
    /// Normalized condition under which the use renders.
    pub condition: GuardDnf,
    /// Kubernetes resource owning the rendered path, when known.
    pub resource: Option<ResourceRef>,
    /// Template locations and helper chains that produced the use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ContractProvenance>,
    /// Go template execution rendered the source through its `%v` spelling,
    /// so a provider slot observes text rather than the raw input shape.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stringified: bool,
    /// Literal member keys the TEMPLATE writes beside this fragment splice
    /// in the same mapping (`- name: tmp` next to `toYaml .Values.tmpVolume`):
    /// the rendered object already has them, so a provider slot's object
    /// requiredness must not re-demand them from the user value.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub template_supplied_member_keys: std::collections::BTreeSet<String>,
    /// Set when the rendered text is one separator-delimited segment of the
    /// source string rather than the raw value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_segment: Option<SplitSegmentUse>,
    /// Set when the value renders as one layer of an ordered `merge`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_layers: Option<MergeLayersUse>,
    /// Set when the rendered text is the collection's RANGE KEY rather than
    /// its value: the sink constrains the key domain only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub range_key: bool,
    /// The rendered text is a Sprig `quote`/`squote` of the value, which
    /// skips nil operands: a missing or null source renders an explicit
    /// YAML null into the sink (see
    /// [`ProviderSchemaUse::nil_omitting`](crate::ProviderSchemaUse::nil_omitting)).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nil_omitting: bool,
    /// Literal member keys a guard-scoped `omit` may remove from the
    /// rendered map before the sink reads it. Each key maps to the sound
    /// RETAIN guards under which the key certainly survives (the omitting
    /// arm certainly did not run); an empty guard list means the key's
    /// survival is undecidable and its sink typing must abstain entirely.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub omitted_members: std::collections::BTreeMap<String, Vec<Guard>>,
    /// Set when the slot renders fresh text DERIVED from the value
    /// (`include … | sha256sum` checksum annotations): the sink observes
    /// neither the value nor its serialization, so the row grants its
    /// branch serialized tolerance without claiming a path-wide
    /// serialization use.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub digest: bool,
    /// Set when the value flowed through a Sprig `merge` call as a DIRECT
    /// operand: the operand's strict map contract rides its own fail
    /// implication (keyed on the call's live gate), so this row never
    /// rejects a Helm-falsy input at the base.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub merge_operand: bool,
}

impl ContractUse {
    /// Creates a contract use from one conjunction of guards.
    #[must_use]
    pub fn new(
        source_expr: ValuesPath,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
    ) -> Self {
        Self::with_provenances(source_expr, path, kind, guards, resource, None)
    }

    /// Creates a guarded contract use with explicit source provenance.
    pub fn with_provenances(
        source_expr: ValuesPath,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
        provenance: impl IntoIterator<Item = ContractProvenance>,
    ) -> Self {
        let condition = GuardDnf::from_guards(guards);
        Self::with_condition_and_provenances(
            source_expr,
            path,
            kind,
            condition,
            resource,
            provenance,
        )
    }

    /// Creates a contract use from an already-normalized condition.
    pub fn with_condition_and_provenances(
        source_expr: ValuesPath,
        path: YamlPath,
        kind: ValueKind,
        condition: GuardDnf,
        resource: Option<ResourceRef>,
        provenance: impl IntoIterator<Item = ContractProvenance>,
    ) -> Self {
        Self {
            source_expr,
            path,
            kind,
            condition,
            resource,
            provenance: provenance.into_iter().collect(),
            stringified: false,
            template_supplied_member_keys: std::collections::BTreeSet::new(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: std::collections::BTreeMap::new(),
            digest: false,
            merge_operand: false,
        }
    }

    /// Sorts and deduplicates provenance without changing semantic evidence.
    pub fn canonicalize(&mut self) {
        self.provenance.sort();
        self.provenance.dedup();
    }

    /// Returns the sole guard conjunction, or an empty conjunction when not singular.
    #[must_use]
    pub fn single_guard_conjunction(&self) -> Vec<Guard> {
        self.condition
            .single_guard_conjunction()
            .unwrap_or_default()
    }

    /// Rewrites the source expression and every values path in the condition.
    pub fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(ValuesPath) -> ValuesPath,
    {
        let Self {
            source_expr,
            path: _,
            kind: _,
            condition,
            resource: _,
            provenance: _,
            stringified: _,
            template_supplied_member_keys: _,
            split_segment: _,
            merge_layers,
            range_key: _,
            nil_omitting: _,
            omitted_members,
            digest: _,
            merge_operand: _,
        } = self;
        *source_expr = map(source_expr.clone());
        condition.map_value_paths(map);
        if let Some(merge) = merge_layers {
            for layer in &mut merge.layers {
                layer.path = map(layer.path.clone());
            }
        }
        for retain_guards in omitted_members.values_mut() {
            for guard in retain_guards {
                *guard = guard.clone().map_value_paths(map);
            }
        }
    }
}
