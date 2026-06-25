use helm_schema_ast::DefineIndex;

use crate::resource_identity::ResourceIdentityIndex;
use crate::{ResourceRef, ValueKind, YamlPath};

mod attribution;
mod yaml_tree;

use attribution::{AttributionIndex, build_attribution_index};

/// Tracks document-local path and resource attribution while the symbolic
/// interpreter walks mixed YAML and Helm actions.
pub(crate) struct DocumentTracker<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    resource_identity: ResourceIdentityIndex,
    attribution: AttributionIndex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutputSlot {
    pub(crate) kind: ValueKind,
    pub(crate) path: YamlPath,
    pub(crate) resource: Option<ResourceRef>,
    pub(crate) slot: OutputSlotKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputSlotKind {
    MappingKey,
    YamlComment,
    WholeScalar,
    PartialScalar,
    FragmentInsertion,
    BlockScalarSuppressed,
    Opaque,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ControlSite {
    pub(crate) path: YamlPath,
    pub(crate) range_mapping_entry_path: Option<YamlPath>,
}

impl OutputSlot {
    pub(crate) fn suppresses_fragment_output(&self) -> bool {
        self.slot == OutputSlotKind::MappingKey
    }

    pub(crate) fn direct_value_kind(&self) -> ValueKind {
        if self.kind == ValueKind::Scalar
            && self.slot == OutputSlotKind::PartialScalar
            && !self.path.0.is_empty()
        {
            ValueKind::PartialScalar
        } else {
            self.kind
        }
    }

    pub(crate) fn direct_value_path(&self, source_expr: &str) -> YamlPath {
        if source_expr.ends_with(".*") && !self.in_sequence_item() {
            YamlPath(Vec::new())
        } else {
            self.path.clone()
        }
    }

    pub(crate) fn can_project_scalar_helper_to_caller_path(&self) -> bool {
        !self.path.0.is_empty()
            && self.kind == ValueKind::Scalar
            && self.slot == OutputSlotKind::WholeScalar
    }

    pub(crate) fn can_project_structured_helper_to_caller_path(&self) -> bool {
        !self.path.0.is_empty()
            && (self.kind == ValueKind::Fragment
                || (self.kind == ValueKind::Scalar && self.slot == OutputSlotKind::WholeScalar))
    }

    pub(crate) fn is_yaml_comment(&self) -> bool {
        self.slot == OutputSlotKind::YamlComment
    }

    fn in_sequence_item(&self) -> bool {
        self.path
            .0
            .last()
            .map(std::string::String::as_str)
            .is_some_and(|segment| segment.ends_with("[*]"))
    }
}

impl<'a> DocumentTracker<'a> {
    pub(crate) fn new(source: &'a str, defines: &'a DefineIndex) -> Self {
        Self {
            source,
            defines,
            resource_identity: ResourceIdentityIndex::default(),
            attribution: AttributionIndex::default(),
        }
    }

    pub(crate) fn reset_for_tree(&mut self, tree: &tree_sitter::Tree) {
        self.resource_identity = ResourceIdentityIndex::from_source(self.source, self.defines);
        self.attribution = build_attribution_index(self.source, tree.root_node());
    }

    pub(crate) fn control_site_for_node(&self, node: tree_sitter::Node<'_>) -> ControlSite {
        self.attribution
            .control_site_for_node(node)
            .unwrap_or_default()
    }

    pub(crate) fn resource_at(&self, byte: usize) -> Option<&ResourceRef> {
        self.resource_identity.resource_at(byte)
    }

    pub(crate) fn rebase_path_at(&self, byte: usize, path: YamlPath) -> YamlPath {
        self.resource_identity.rebase_path_at(byte, path)
    }

    pub(crate) fn output_slot_for_action(&self, node: tree_sitter::Node<'_>) -> OutputSlot {
        let mut slot = self
            .attribution
            .output_slot_for_node(node)
            .unwrap_or_else(|| OutputSlot {
                kind: ValueKind::Scalar,
                path: YamlPath(Vec::new()),
                resource: None,
                slot: OutputSlotKind::Opaque,
            });
        slot.path = self
            .resource_identity
            .rebase_path_at(node.start_byte(), slot.path);
        slot.resource = self
            .resource_identity
            .resource_at(node.start_byte())
            .cloned();
        slot
    }
}

#[cfg(test)]
#[path = "../../tests/document_projection/tracker/mod.rs"]
mod tests;
