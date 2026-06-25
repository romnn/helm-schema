use helm_schema_ast::{
    AttributionIndex, ControlSite, DefineIndex, OutputSlot, OutputSlotKind, ResourceIdentityIndex,
    build_attribution_index,
};

use crate::{ResourceRef, ValueKind, YamlPath};

/// Tracks document-local path and resource attribution while the symbolic
/// interpreter walks mixed YAML and Helm actions.
pub(crate) struct DocumentTracker<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    resource_identity: ResourceIdentityIndex,
    attribution: AttributionIndex,
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
