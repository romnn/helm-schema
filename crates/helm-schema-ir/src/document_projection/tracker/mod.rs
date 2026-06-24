use helm_schema_ast::DefineIndex;

use crate::resource_identity::ResourceIdentityIndex;
use crate::{ResourceRef, SourceSpan, ValueKind, YamlPath};

mod attribution;
mod yaml_tree;

use attribution::{AttributionIndex, ResolvedNodeContext, build_attribution_index};

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
    pub(crate) in_mapping_key: bool,
    pub(crate) in_yaml_comment: bool,
    pub(crate) entire_scalar_value: bool,
    pub(crate) source_span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObservedOutputSite {
    pub(crate) kind: ValueKind,
    pub(crate) path: YamlPath,
}

impl OutputSlot {
    pub(crate) fn fragment_output_site(&self) -> Option<ObservedOutputSite> {
        if self.in_mapping_key {
            return None;
        }

        Some(ObservedOutputSite {
            kind: self.direct_value_kind(),
            path: self.path.clone(),
        })
    }

    pub(crate) fn direct_value_kind(&self) -> ValueKind {
        if self.kind == ValueKind::Scalar && !self.entire_scalar_value && !self.path.0.is_empty() {
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
        !self.in_mapping_key
            && !self.path.0.is_empty()
            && self.kind == ValueKind::Scalar
            && self.entire_scalar_value
    }

    pub(crate) fn can_project_structured_helper_to_caller_path(&self) -> bool {
        !self.in_mapping_key
            && !self.path.0.is_empty()
            && (self.kind == ValueKind::Fragment
                || (self.kind == ValueKind::Scalar && self.entire_scalar_value))
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

    fn context_for_node(&self, node: tree_sitter::Node<'_>) -> ResolvedNodeContext {
        if matches!(node.kind(), "if_action" | "with_action" | "range_action") {
            self.attribution
                .control_context_for_node(node)
                .unwrap_or_default()
        } else {
            ResolvedNodeContext::default()
        }
    }

    pub(crate) fn path_for_node(&self, node: tree_sitter::Node<'_>) -> YamlPath {
        let context = self.context_for_node(node);
        if context.inside_block_scalar {
            return YamlPath(Vec::new());
        }

        context.current_path
    }

    pub(crate) fn path_at_mapping_entry_indent(
        &self,
        node: tree_sitter::Node<'_>,
        indent: usize,
    ) -> YamlPath {
        let context = self.context_for_node(node);
        if context.inside_block_scalar {
            return YamlPath(Vec::new());
        }

        if let Some(context) = self.attribution.mapping_entry_context_in_span_at_indent(
            node.start_byte(),
            node.end_byte(),
            indent,
        ) {
            return context.mapping_entry_path;
        }

        context.mapping_entry_path
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
                in_mapping_key: false,
                in_yaml_comment: false,
                entire_scalar_value: false,
                source_span: SourceSpan::new(node.start_byte(), node.end_byte()),
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
