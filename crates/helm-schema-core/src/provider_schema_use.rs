use crate::{ResourceRef, ValueKind, YamlPath};

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
}
