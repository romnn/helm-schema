use helm_schema_ast::DefineIndex;

use crate::fragment_classification::is_fragment_expr;
use crate::resource_identity::ResourceIdentityIndex;
use crate::yaml_shape::Shape;
use crate::{ResourceRef, YamlPath};

mod fragment_indent;
mod inline_mapping;
mod source_position;
mod text_ingest;

use fragment_indent::fragment_indent_width;
use source_position::{
    line_indent_and_col, source_position_is_inside_block_scalar, starts_template_action_line,
};
use text_ingest::TextIngestState;

/// Tracks source-position-dependent rendered YAML state while a template AST is
/// walked.
///
/// The symbolic walker decides what a Helm expression means. This context owns
/// the independent question of where that expression lands in rendered YAML and
/// which Kubernetes resource span contains it.
pub(crate) struct RenderedYamlContext<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    shape: Shape,
    output_inside_block_scalar: bool,
    resource_identity: ResourceIdentityIndex,
    text_ingest: TextIngestState,
}

impl<'a> RenderedYamlContext<'a> {
    pub(crate) fn new(source: &'a str, defines: &'a DefineIndex) -> Self {
        Self {
            source,
            defines,
            shape: Shape::default(),
            output_inside_block_scalar: false,
            resource_identity: ResourceIdentityIndex::default(),
            text_ingest: TextIngestState::default(),
        }
    }

    pub(crate) fn reset_for_tree(&mut self, tree: &tree_sitter::Tree) {
        self.text_ingest.reset_for_tree(tree);
        self.resource_identity = ResourceIdentityIndex::from_source(self.source, self.defines);
        self.shape = Shape::default();
        self.output_inside_block_scalar = false;
    }

    pub(crate) fn enter_node(&mut self, node: tree_sitter::Node<'_>) {
        self.ingest_text_up_to(node.start_byte());
        self.resource_identity.advance_to(node.start_byte());
        self.sync_action_for_node(node);
    }

    pub(crate) fn current_path(&self) -> YamlPath {
        self.shape.current_path()
    }

    pub(crate) fn current_resource(&self) -> Option<&ResourceRef> {
        self.resource_identity.current_resource()
    }

    pub(crate) fn rebase_path(&self, path: YamlPath) -> YamlPath {
        self.resource_identity.rebase_path(path)
    }

    pub(crate) fn trailing_pending_mapping_segments_at_or_above(&self, indent: usize) -> usize {
        self.shape
            .trailing_pending_mapping_segments_at_or_above(indent)
    }

    pub(crate) fn output_inside_block_scalar_at(&self, byte_pos: usize) -> bool {
        let (indent, _) = self.line_indent_and_col(byte_pos);
        self.output_inside_block_scalar
            || self.source_position_is_inside_block_scalar(byte_pos, indent)
    }

    pub(crate) fn inline_mapping_value_path(
        &self,
        node: tree_sitter::Node<'_>,
    ) -> Option<YamlPath> {
        inline_mapping::inline_mapping_value_path(self.source, &self.shape, node)
    }

    pub(crate) fn ingest_text_up_to(&mut self, target: usize) {
        self.text_ingest
            .ingest_text_up_to(self.source, &mut self.shape, target);
    }

    pub(crate) fn line_indent_and_col(&self, byte_pos: usize) -> (usize, usize) {
        line_indent_and_col(self.source, byte_pos)
    }

    pub(crate) fn starts_template_action_line(&self, byte_pos: usize) -> bool {
        starts_template_action_line(self.source, byte_pos)
    }

    pub(crate) fn fragment_indent_width(text: &str) -> Option<usize> {
        fragment_indent_width(text)
    }

    fn source_position_is_inside_block_scalar(&self, byte_pos: usize, indent: usize) -> bool {
        source_position_is_inside_block_scalar(self.source, byte_pos, indent)
    }

    fn sync_action_for_node(&mut self, node: tree_sitter::Node<'_>) {
        if matches!(node.kind(), "text" | "yaml_no_injection_text") {
            return;
        }

        // Control actions do not emit YAML structure, so they must not mutate
        // the rendered shape stack.
        if !matches!(node.kind(), "template_action" | "variable") {
            return;
        }

        let mut pos = node.start_byte().min(self.source.len());
        let end = node.end_byte().min(self.source.len());
        while pos < end {
            match self.source.as_bytes()[pos] {
                b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
                _ => break,
            }
        }

        if pos > node.start_byte() {
            let leading = &self.source[node.start_byte()..pos];
            let mut sanitized = String::with_capacity(leading.len());
            for ch in leading.chars() {
                if ch == '\n' || ch == ' ' || ch == '\t' {
                    sanitized.push(ch);
                }
            }
            if !sanitized.is_empty() {
                self.shape.ingest(&sanitized);
                self.text_ingest.set_position(pos);
            }
        }

        let (physical_indent, physical_col) = self.line_indent_and_col(pos);
        let shape_inside_block_scalar = self.shape.is_inside_block_scalar_line(physical_indent);
        let source_inside_block_scalar =
            self.source_position_is_inside_block_scalar(pos, physical_indent);
        self.output_inside_block_scalar = shape_inside_block_scalar || source_inside_block_scalar;

        let allow_clear_pending = if node.kind() == "template_action" {
            if let Ok(text) = node.utf8_text(self.source.as_bytes()) {
                !is_fragment_expr(text)
            } else {
                true
            }
        } else {
            false
        };

        let (indent, col) = if node.kind() == "template_action" && !allow_clear_pending {
            if let Ok(text) = node.utf8_text(self.source.as_bytes())
                && let Some(virtual_indent) = fragment_indent_width(text)
                && virtual_indent > physical_indent
            {
                (virtual_indent, virtual_indent)
            } else {
                (physical_indent, physical_col)
            }
        } else {
            (physical_indent, physical_col)
        };

        self.shape
            .sync_action_position(indent, col, allow_clear_pending);
    }
}
