use helm_schema_ast::TemplateExpr;

use crate::contract::ContractUse;
use crate::contract_sink::ContractUseContext;
use crate::template_expr_cache::parse_expr_text;
use crate::{Guard, ResourceRef, SourceSpan, ValueKind, YamlPath};

use super::tracker::DocumentTracker;

pub(crate) struct DocumentSiteContext {
    pub(crate) kind: ValueKind,
    pub(crate) in_mapping_key: bool,
    pub(crate) in_yaml_comment: bool,
    pub(crate) entire_scalar_value: bool,
    pub(crate) path: YamlPath,
    pub(crate) resource: Option<ResourceRef>,
    pub(crate) source_span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObservedOutputSite {
    pub(crate) kind: ValueKind,
    pub(crate) path: YamlPath,
}

impl DocumentSiteContext {
    pub(crate) fn fragment_output_site(&self) -> Option<ObservedOutputSite> {
        if self.in_mapping_key {
            return None;
        }

        let kind = if self.kind == ValueKind::Scalar
            && !self.entire_scalar_value
            && !self.path.0.is_empty()
        {
            ValueKind::PartialScalar
        } else {
            self.kind
        };

        Some(ObservedOutputSite {
            kind,
            path: self.path.clone(),
        })
    }

    pub(crate) fn contract_use(
        &self,
        context: &ContractUseContext<'_>,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
    ) -> ContractUse {
        context.contract_use(source_expr, path, kind, &guards, self.resource.clone())
    }

    pub(crate) fn contract_use_with_extra_provenance(
        &self,
        context: &ContractUseContext<'_>,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        extra_provenance: &[crate::ContractProvenance],
    ) -> ContractUse {
        context.contract_use_with_extra_provenance(
            source_expr,
            path,
            kind,
            &guards,
            self.resource.clone(),
            extra_provenance,
        )
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

pub(crate) fn collect_document_site_context(
    source: &str,
    tracker: &DocumentTracker<'_>,
    node: tree_sitter::Node<'_>,
    exprs: &[TemplateExpr],
) -> DocumentSiteContext {
    let output_action = analyze_output_action(source, node, exprs);
    let kind = if output_action.is_fragment {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };

    let slot = tracker.output_slot_for_node(node, kind, output_action.fragment_indent_width);

    DocumentSiteContext {
        kind,
        in_mapping_key: slot.in_mapping_key,
        in_yaml_comment: document_site_is_yaml_comment_part(source, node),
        entire_scalar_value: slot.entire_scalar_value,
        path: slot.path,
        resource: slot.resource,
        source_span: SourceSpan::new(node.start_byte(), node.end_byte()),
    }
}

struct OutputActionAnalysis {
    is_fragment: bool,
    fragment_indent_width: Option<usize>,
}

fn analyze_output_action(
    source: &str,
    node: tree_sitter::Node<'_>,
    exprs: &[TemplateExpr],
) -> OutputActionAnalysis {
    if node.kind() == "template_action" {
        return output_action_shape_from_exprs(exprs);
    }

    if let Some(text) = enclosing_action_text(source, node) {
        return output_action_shape_from_exprs(&parse_expr_text(&text));
    }

    output_action_shape_from_exprs(exprs)
}

fn output_action_shape_from_exprs(exprs: &[TemplateExpr]) -> OutputActionAnalysis {
    OutputActionAnalysis {
        is_fragment: exprs.iter().any(TemplateExpr::renders_yaml_fragment),
        fragment_indent_width: DocumentTracker::fragment_indent_width_for_exprs(exprs),
    }
}

fn enclosing_action_text(source: &str, node: tree_sitter::Node<'_>) -> Option<String> {
    let mut current = node;
    loop {
        match current.kind() {
            "template_action" => {
                return current
                    .utf8_text(source.as_bytes())
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

fn document_site_is_yaml_comment_part(source: &str, node: tree_sitter::Node<'_>) -> bool {
    let start = node.start_byte();
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    source[line_start..start].trim_start().starts_with('#')
}

#[cfg(test)]
mod tests {
    use test_util::prelude::sim_assert_eq;

    use super::{DocumentSiteContext, ObservedOutputSite};
    use crate::{SourceSpan, ValueKind, YamlPath};

    #[test]
    fn fragment_output_site_suppresses_mapping_keys() {
        let site = DocumentSiteContext {
            kind: ValueKind::Scalar,
            in_mapping_key: true,
            in_yaml_comment: false,
            entire_scalar_value: true,
            path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            resource: None,
            source_span: SourceSpan::new(0, 0),
        };

        sim_assert_eq!(have: site.fragment_output_site(), want: None);
    }

    #[test]
    fn fragment_output_site_marks_partial_scalar_slots() {
        let site = DocumentSiteContext {
            kind: ValueKind::Scalar,
            in_mapping_key: false,
            in_yaml_comment: false,
            entire_scalar_value: false,
            path: YamlPath(vec!["spec".to_string(), "value".to_string()]),
            resource: None,
            source_span: SourceSpan::new(0, 0),
        };

        sim_assert_eq!(
            have: site.fragment_output_site(),
            want: Some(ObservedOutputSite {
                kind: ValueKind::PartialScalar,
                path: YamlPath(vec!["spec".to_string(), "value".to_string()]),
            })
        );
    }
}
