use crate::{MergeLayersUse, ResourceRef, SplitSegmentUse, ValueKind, YamlPath};

/// Contract fact that needs a Kubernetes resource schema lookup.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderSchemaUse {
    pub value_path: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub resource: ResourceRef,
    pub is_self_range_collection: bool,
    /// Literal member keys the template writes beside the splice in the
    /// same mapping; the slot schema's `required` must not re-demand them.
    pub template_supplied_member_keys: std::collections::BTreeSet<String>,
    /// Set when the rendered text is one separator-delimited segment of the
    /// source string; the slot schema constrains that segment only.
    pub split_segment: Option<SplitSegmentUse>,
    /// Set when the value renders as one layer of an ordered `merge`: a
    /// shadowed layer's members reach the slot only where every earlier
    /// layer lacks them.
    pub merge_layers: Option<MergeLayersUse>,
    /// Set when the rendered text is the collection's RANGE KEY rather than
    /// its value: a string-only slot then excludes the integer keys of a
    /// non-empty list lane.
    pub range_key: bool,
    /// Literal member keys a guard-scoped `omit` may remove from the
    /// rendered map before the sink reads it: the slot's whole-payload
    /// typing must exclude them, and each key's member typing is re-added
    /// only under its RETAIN guards (empty means never).
    pub omitted_members: std::collections::BTreeMap<String, Vec<crate::ConditionalGuard>>,
    /// Decoded conditions gating the render this use rides. Synthesized
    /// merge-layer arms must carry them: without the gates a dormant state
    /// (KPS's `defaultRules.create: false`) would still be typed by the
    /// layer arms even though nothing renders.
    pub outer_guards: Vec<crate::ConditionalGuard>,
}
