mod capability_branch;
pub mod expr;
mod expr_function_catalog;
mod literal_schema_type;
mod printf_eval;
mod range_structure;
mod resource_span;
mod semver_constraint;
mod template_action;
mod tree_sitter_utils;
mod values_comments;

pub use capability_branch::{decode_guard, decode_guard_expr};
pub use expr::unconditional_include_names;
pub use expr::{Literal, TemplateExpr, parse_action_expressions};
pub use expr_function_catalog::{
    go_type_descriptor_spellings, go_type_schema_type, is_checksum_function,
    is_coercing_arithmetic_function, is_merge_function, is_provenance_preserving_function,
    is_string_predicate_function, is_string_splitting_function, is_string_transform_function,
    is_total_numeric_cast_function, is_total_stringification_function,
    strict_collection_item_pattern, strict_parser_operand_pattern, string_operand_indices,
    type_descriptor_call_subject, type_is_schema_type,
};
pub(crate) use helm_schema_syntax::structural_mapping_colon;
pub use helm_schema_syntax::{parse_yaml_key, unquote_yaml_scalar};
pub use literal_schema_type::expression_schema_type;
pub use printf_eval::{literal_printf_format, render_printf_string_sets};
pub use range_structure::{
    range_destructured_key_variable, range_destructured_value_variable,
    range_has_destructured_variable_definition, range_header_from_source, range_variable_name_expr,
};
pub use resource_span::{KindBranchSource, ResourceSpan};
pub use semver_constraint::semver_constraint_match_pattern;
pub use template_action::contains_template_action;
pub use tree_sitter_utils::{
    children_with_field, parse_expr_text, parse_go_template, parse_helm_template,
};
pub use values_comments::extract_values_yaml_descriptions;

use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("tree-sitter parse failed")]
    TreeSitterParseFailed,
}

/// Parsed control-flow header from a Helm template node.
///
/// Helm control-flow remains source-preserving for rendering/inspection while
/// also exposing a typed expression so structural consumers do not have to
/// re-parse `if` / `with` / `range` headers from raw strings.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateHeader {
    raw: String,
    expr: TemplateExpr,
}

impl TemplateHeader {
    fn new(raw: impl Into<String>, expr: TemplateExpr) -> Self {
        Self {
            raw: raw.into(),
            expr,
        }
    }

    #[must_use]
    pub fn parse_control(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let expr = parse_control_expr(&raw).unwrap_or_else(|| TemplateExpr::Unknown(raw.clone()));
        Self::new(raw, expr)
    }

    #[must_use]
    pub(crate) fn parse_range(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let wrapped = format!("{{{{ range {raw} }}}}{{{{ end }}}}");
        let expr = parse_action_expressions(&wrapped)
            .into_iter()
            .next()
            .unwrap_or_else(|| TemplateExpr::Unknown(raw.clone()));
        Self::new(raw, expr)
    }

    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub fn expr(&self) -> &TemplateExpr {
        &self.expr
    }
}

fn parse_control_expr(raw: &str) -> Option<TemplateExpr> {
    let parsed = parse_action_expressions(&format!("{{{{ {raw} }}}}"))
        .into_iter()
        .next();
    match parsed {
        Some(TemplateExpr::Unknown(_)) | None => {
            let condition = normalized_control_condition(raw)?;
            parse_action_expressions(&format!("{{{{ {condition} }}}}"))
                .into_iter()
                .next()
        }
        expr => expr,
    }
}

fn normalized_control_condition(raw: &str) -> Option<&str> {
    let mut text = raw.trim();
    if let Some(rest) = text.strip_prefix("{{") {
        text = rest.trim_start();
        if let Some(rest) = text.strip_prefix('-') {
            text = rest.trim_start();
        }
        let rest = text.strip_suffix("}}")?;
        text = rest.trim_end();
        if let Some(rest) = text.strip_suffix('-') {
            text = rest.trim_end();
        }
    }

    text.strip_prefix("else if ")
        .or_else(|| text.strip_prefix("if "))
        .or_else(|| text.strip_prefix("with "))
}

/// Index of helper/template file sources.
#[derive(Default, Debug, Clone)]
pub struct DefineIndex {
    files: HashMap<String, String>,
}

impl DefineIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file_source(&mut self, path: &str, src: &str) {
        self.files.retain(|existing_path, existing_src| {
            !is_inline_source(existing_path, existing_src, src)
        });
        self.files.insert(path.to_string(), src.to_string());
    }

    #[must_use]
    pub fn get_file(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(std::string::String::as_str)
    }

    pub fn file_sources(&self) -> impl Iterator<Item = (&str, &str)> {
        let mut entries: Vec<_> = self.files.iter().collect();
        entries.sort_by_key(|(path, _)| *path);
        entries
            .into_iter()
            .map(|(path, src)| (path.as_str(), src.as_str()))
    }
}

fn is_inline_source(path: &str, existing_src: &str, src: &str) -> bool {
    path.starts_with("<inline:") && path.ends_with('>') && existing_src == src
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
