use helm_schema_ast::{DefineIndex, TemplateExpr};

use crate::resource_identity::ResourceIdentityIndex;
use crate::template_expr_cache::parse_expr_text;
use crate::{ResourceRef, SourceSpan, ValueKind, YamlPath};

mod attribution;
mod yaml_tree;

use attribution::{
    AttributionIndex, ResolvedNodeContext, build_attribution_index, is_output_root_kind,
};

/// Tracks document-local path and resource attribution while the symbolic
/// interpreter walks mixed YAML and Helm actions.
pub(crate) struct DocumentTracker<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    resource_identity: ResourceIdentityIndex,
    attribution: AttributionIndex,
}

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

struct OutputActionAnalysis {
    is_fragment: bool,
    fragment_indent_width: Option<usize>,
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
        if is_output_root_kind(node.kind()) {
            self.attribution
                .output_context_for_node(node)
                .unwrap_or_default()
        } else if matches!(node.kind(), "if_action" | "with_action" | "range_action") {
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

    pub(crate) fn output_slot_for_action(
        &self,
        node: tree_sitter::Node<'_>,
        exprs: &[TemplateExpr],
    ) -> OutputSlot {
        let output_action = self.analyze_output_action(node, exprs);
        let kind = if output_action.is_fragment {
            ValueKind::Fragment
        } else {
            ValueKind::Scalar
        };

        self.output_slot_for_node(node, kind, output_action.fragment_indent_width)
    }

    pub(crate) fn output_slot_for_node(
        &self,
        node: tree_sitter::Node<'_>,
        kind: ValueKind,
        fragment_indent_width: Option<usize>,
    ) -> OutputSlot {
        let current_context = self.context_for_node(node);
        let path = if current_context.in_mapping_key {
            YamlPath(Vec::new())
        } else {
            self.output_site_path_from_context(node, kind, fragment_indent_width, &current_context)
        };

        OutputSlot {
            kind,
            path: self
                .resource_identity
                .rebase_path_at(node.start_byte(), path),
            resource: self
                .resource_identity
                .resource_at(node.start_byte())
                .cloned(),
            in_mapping_key: current_context.in_mapping_key,
            in_yaml_comment: self.document_site_is_yaml_comment_part(node),
            entire_scalar_value: current_context.entire_scalar_value,
            source_span: SourceSpan::new(node.start_byte(), node.end_byte()),
        }
    }

    fn output_site_path_from_context(
        &self,
        node: tree_sitter::Node<'_>,
        kind: ValueKind,
        fragment_indent_width: Option<usize>,
        current_context: &ResolvedNodeContext,
    ) -> YamlPath {
        if current_context.inside_block_scalar {
            return YamlPath(Vec::new());
        }

        let mut path = if kind == ValueKind::Fragment {
            let rendered_context = fragment_indent_width.and_then(|indent| {
                self.attribution
                    .virtual_indent_context_for_node(node, indent)
            });
            prefer_fragment_output_path(current_context, rendered_context.as_ref())
        } else {
            current_context.output_path.clone()
        };
        if kind == ValueKind::Fragment
            && let Some(last) = path.0.last_mut()
            && let Some(stripped) = last.strip_suffix("[*]")
        {
            *last = stripped.to_string();
        }
        path
    }

    pub(crate) fn fragment_indent_width_for_exprs(exprs: &[TemplateExpr]) -> Option<usize> {
        exprs
            .iter()
            .rev()
            .find_map(TemplateExpr::fragment_indent_width)
    }

    fn analyze_output_action(
        &self,
        node: tree_sitter::Node<'_>,
        exprs: &[TemplateExpr],
    ) -> OutputActionAnalysis {
        if node.kind() == "template_action" {
            return Self::output_action_shape_from_exprs(exprs);
        }

        if let Some(text) = self.enclosing_action_text(node) {
            return Self::output_action_shape_from_exprs(&parse_expr_text(&text));
        }

        Self::output_action_shape_from_exprs(exprs)
    }

    fn output_action_shape_from_exprs(exprs: &[TemplateExpr]) -> OutputActionAnalysis {
        OutputActionAnalysis {
            is_fragment: exprs.iter().any(TemplateExpr::renders_yaml_fragment),
            fragment_indent_width: Self::fragment_indent_width_for_exprs(exprs),
        }
    }

    fn enclosing_action_text(&self, node: tree_sitter::Node<'_>) -> Option<String> {
        let mut current = node;
        loop {
            match current.kind() {
                "template_action" => {
                    return current
                        .utf8_text(self.source.as_bytes())
                        .ok()
                        .map(std::string::ToString::to_string);
                }
                "if_action" | "with_action" | "range_action" => return None,
                _ => {
                    current = current.parent()?;
                }
            }
        }
    }

    fn document_site_is_yaml_comment_part(&self, node: tree_sitter::Node<'_>) -> bool {
        let start = node.start_byte();
        let line_start = self.source[..start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        self.source[line_start..start].trim_start().starts_with('#')
    }
}

fn prefer_fragment_output_path(
    current: &ResolvedNodeContext,
    rendered: Option<&ResolvedNodeContext>,
) -> YamlPath {
    let current_path = &current.output_path;
    let Some(rendered) = rendered else {
        return current_path.clone();
    };
    let rendered_path = &rendered.output_path;
    if current_path.0.is_empty() {
        return rendered_path.clone();
    }
    if rendered_path.0.is_empty() {
        return current_path.clone();
    }
    if path_is_prefix_of(&rendered_path.0, &current_path.0) {
        if rendered.sequence_item_slot {
            return rendered_path.clone();
        }
        return current_path.clone();
    }
    if path_is_prefix_of(&current_path.0, &rendered_path.0) {
        return preserve_specific_prefix(current_path, rendered_path);
    }
    rendered_path.clone()
}

fn preserve_specific_prefix(prefix: &YamlPath, path: &YamlPath) -> YamlPath {
    if prefix.0.is_empty() || prefix.0.len() > path.0.len() {
        return path.clone();
    }

    let mut merged = prefix.0.clone();
    merged.extend(path.0.iter().skip(prefix.0.len()).cloned());
    YamlPath(merged)
}

fn path_is_prefix_of(prefix: &[String], path: &[String]) -> bool {
    prefix.len() <= path.len()
        && prefix
            .iter()
            .zip(path)
            .all(|(left, right)| path_segments_equivalent(left, right))
}

fn path_segments_equivalent(left: &str, right: &str) -> bool {
    left == right
        || left.strip_suffix("[*]") == Some(right)
        || right.strip_suffix("[*]") == Some(left)
}

#[cfg(test)]
#[path = "../../tests/document_projection/tracker/mod.rs"]
mod tests;
