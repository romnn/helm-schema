use serde::{Deserialize, Serialize};

use crate::{ContractProvenance, Guard, GuardDnf, ResourceRef, ValueKind, YamlPath};

/// The rendered text is ONE SEGMENT of the source string split by a literal
/// separator (`regexSplit ":" . -1 | last` extracting a port suffix): the
/// sink schema constrains that segment, never the whole raw value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SplitSegmentUse {
    pub separator: String,
    /// The LAST segment when true, the first otherwise.
    pub last: bool,
}

/// The value is one layer of an ordered Sprig `merge`: a key of an earlier
/// layer shadows the same key of every later layer at the rendered sink, so
/// a later layer's member reaches the sink only where every earlier layer
/// lacks that member.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MergeLayersUse {
    /// Every layer's values path, highest precedence first.
    pub layers: Vec<String>,
    /// This use's own index within `layers`.
    pub position: usize,
    /// Per-layer scrub markers, parallel to `layers`: a `true` layer's map
    /// had nil members recursively removed before the merge (airflow's
    /// `removeNilFields`), so that layer's sink typing must admit null
    /// member spellings, and binding-carried rows of a scrub-involving
    /// merge keep the layered routing.
    pub nil_scrubbed_layers: Vec<bool>,
    /// The layer facts were recovered from a local BINDING's meta (the
    /// merged value flowed through a helper output before rendering)
    /// rather than the render site's own layered value. Such rows keep
    /// their base metadata field kind: the binding's other dispatch arms
    /// (bitnami's `tplvalues.render` string lane) rely on the string-map
    /// alternative it contributes, while direct render-site layers moved
    /// that typing onto the synthesized arms entirely.
    pub via_binding: bool,
}

impl MergeLayersUse {
    /// The higher-precedence layer paths whose keys shadow this layer's.
    #[must_use]
    pub fn shadowed_by(&self) -> &[String] {
        &self.layers[..self.position.min(self.layers.len())]
    }
}

/// A contract claim for one observed values path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ContractUse {
    pub source_expr: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub condition: GuardDnf,
    pub resource: Option<ResourceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ContractProvenance>,
    /// A string-consuming transform (`trunc`, `b64enc`, a dynamic `printf`
    /// format) produced this rendered text: rendering fails for non-string
    /// values, but only where THIS row's condition holds.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_string_contract: bool,
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

impl<'de> Deserialize<'de> for ContractUse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireContractUse {
            source_expr: String,
            path: YamlPath,
            kind: ValueKind,
            condition: GuardDnf,
            resource: Option<ResourceRef>,
            #[serde(default)]
            provenance: Vec<ContractProvenance>,
            #[serde(default)]
            has_string_contract: bool,
            #[serde(default)]
            template_supplied_member_keys: std::collections::BTreeSet<String>,
            #[serde(default)]
            split_segment: Option<SplitSegmentUse>,
            #[serde(default)]
            merge_layers: Option<MergeLayersUse>,
            #[serde(default)]
            range_key: bool,
            #[serde(default)]
            omitted_members: std::collections::BTreeMap<String, Vec<Guard>>,
            #[serde(default)]
            digest: bool,
            #[serde(default)]
            merge_operand: bool,
        }

        let wire = WireContractUse::deserialize(deserializer)?;
        Ok(Self {
            source_expr: wire.source_expr,
            path: wire.path,
            kind: wire.kind,
            condition: wire.condition,
            resource: wire.resource,
            provenance: wire.provenance,
            has_string_contract: wire.has_string_contract,
            template_supplied_member_keys: wire.template_supplied_member_keys,
            split_segment: wire.split_segment,
            merge_layers: wire.merge_layers,
            range_key: wire.range_key,
            omitted_members: wire.omitted_members,
            digest: wire.digest,
            merge_operand: wire.merge_operand,
        })
    }
}

impl ContractUse {
    pub fn new(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
    ) -> Self {
        Self::with_provenances(source_expr, path, kind, guards, resource, None)
    }

    pub fn with_provenances(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
        provenance: impl IntoIterator<Item = ContractProvenance>,
    ) -> Self {
        let condition = GuardDnf::from_guards(guards.iter().cloned());
        Self::with_condition_and_provenances(
            source_expr,
            path,
            kind,
            condition,
            resource,
            provenance,
        )
    }

    pub fn with_condition_and_provenances(
        source_expr: String,
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
            has_string_contract: false,
            template_supplied_member_keys: std::collections::BTreeSet::new(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: std::collections::BTreeMap::new(),
            digest: false,
            merge_operand: false,
        }
    }

    pub fn canonicalize(&mut self) {
        self.provenance.sort();
        self.provenance.dedup();
    }

    #[must_use]
    pub fn single_guard_conjunction(&self) -> Vec<Guard> {
        self.condition
            .single_guard_conjunction()
            .unwrap_or_default()
    }

    pub fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        self.source_expr = map(&self.source_expr);
        self.condition.map_value_paths(map);
    }
}
