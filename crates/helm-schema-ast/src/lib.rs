//! Typed structural analysis for Helm templates and templated YAML.

mod capability_branch;
/// Typed Go-template expression nodes and expression parsing.
pub mod expr;
mod printf_eval;
mod range_structure;
mod resource_span;
mod semver_constraint;
mod template_action;
mod tree_sitter_utils;
mod values_comments;

pub use capability_branch::{decode_guard_expr, decode_header_guard};
pub use expr::unconditional_include_names;
pub use expr::{
    Literal, ParsedAction, ParsedActions, TemplateExpr, node_expressions, parse_action_expressions,
    span_expressions,
};
pub(crate) use helm_schema_syntax::structural_mapping_colon;
pub use helm_schema_syntax::{parse_yaml_key, unquote_yaml_scalar};
pub use printf_eval::{PrintfPart, PrintfVerb, parse_supported_printf_format};
pub use range_structure::{
    range_destructured_key_variable, range_destructured_value_variable,
    range_has_destructured_variable_definition, range_header_from_source, range_uses_assignment,
    range_variable_name_expr,
};
pub use resource_span::{KindBranchSource, ResourceSpan};
pub use semver_constraint::{
    SemverParts, parse_semver_version, semver_constraint_match_pattern,
    semver_constraint_matches_version,
};
pub use template_action::contains_template_action;
pub use tree_sitter_utils::{children_with_field, parse_expr_text, parse_go_template};
pub use values_comments::extract_values_yaml_descriptions;

use std::collections::HashMap;

/// Errors produced while parsing Helm template structure.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// Tree-sitter could not produce a syntax tree.
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

    /// Builds a control header from its already-parsed node: an `if` or
    /// `with` condition, or a `range` clause. A node the grammar could not
    /// convert keeps its source text as an unknown expression.
    #[must_use]
    pub fn from_node(node: tree_sitter::Node<'_>, source: &str) -> Self {
        let raw = node
            .utf8_text(source.as_bytes())
            .unwrap_or_default()
            .trim()
            .to_string();
        let expr = match expr::node_expressions(node, source).into_iter().next() {
            Some(TemplateExpr::Unknown(_)) | None => TemplateExpr::Unknown(raw.clone()),
            Some(expr) => expr,
        };
        Self::new(raw, expr)
    }

    /// Returns the source-preserving control header text.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Returns the parsed expression carried by the header.
    #[must_use]
    pub fn expr(&self) -> &TemplateExpr {
        &self.expr
    }
}

/// What an indexed chart file source is to Helm.
///
/// Helm keeps two disjoint namespaces: templates, which it registers under
/// an execution name and can execute, and chart files, which only
/// `.Files.Get` can read. The role records which one a source belongs to
/// so consumers never have to recover it from the path text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefineSourceRole {
    /// A template registered under its Helm execution name.
    Template,
    /// A static chart file reachable only through `.Files.Get`.
    FilesGet,
}

/// Index of helper/template file sources.
#[derive(Default, Debug, Clone)]
pub struct DefineIndex {
    files: HashMap<String, IndexedSource>,
    chart_files: std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<u8>>>,
}

#[derive(Debug, Clone)]
struct IndexedSource {
    role: DefineSourceRole,
    src: String,
    chart_instance: Option<String>,
}

impl DefineIndex {
    /// Creates an empty index of chart and helper sources.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces one template source, keyed by its execution name.
    pub fn add_file_source(&mut self, path: &str, src: &str) {
        self.insert(path, src, DefineSourceRole::Template);
    }

    /// Adds a static file to the unqualified chart instance used by standalone templates.
    pub fn add_files_get_source(&mut self, path: &str, src: &str) {
        self.add_chart_file("", path, src.as_bytes());
    }

    /// Adds a frozen file payload under its chart instance and chart-relative name.
    pub fn add_chart_file(&mut self, chart_instance: &str, path: &str, bytes: &[u8]) {
        self.chart_files
            .entry(chart_instance.to_owned())
            .or_default()
            .insert(path.to_owned(), bytes.to_vec());
        if let Ok(source) = std::str::from_utf8(bytes) {
            let qualified = if chart_instance.is_empty() {
                path.to_owned()
            } else {
                format!("{chart_instance}/{path}")
            };
            self.insert(&qualified, source, DefineSourceRole::FilesGet);
        }
    }

    /// The frozen static file catalogs keyed by chart instance and relative file name.
    #[must_use]
    pub fn chart_files(
        &self,
    ) -> &std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<u8>>> {
        &self.chart_files
    }

    /// Registers a template with the chart instance supplying its Helm root context.
    pub fn add_chart_template_source(&mut self, chart_instance: &str, path: &str, src: &str) {
        self.add_file_source(path, src);
        if let Some(entry) = self.files.get_mut(path) {
            entry.chart_instance = Some(chart_instance.to_owned());
        }
    }

    /// Returns the chart instance owning a registered template.
    #[must_use]
    pub fn chart_instance(&self, path: &str) -> Option<&str> {
        self.files.get(path)?.chart_instance.as_deref()
    }

    fn insert(&mut self, path: &str, src: &str, role: DefineSourceRole) {
        self.files
            .retain(|existing_path, existing| !is_inline_source(existing_path, &existing.src, src));
        self.files.insert(
            path.to_string(),
            IndexedSource {
                role,
                src: src.to_string(),
                chart_instance: None,
            },
        );
    }

    /// Returns the source registered at a logical chart path.
    #[must_use]
    pub fn get_file(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(|entry| entry.src.as_str())
    }

    /// Iterates logical chart paths, roles and sources in stable path order.
    pub fn file_sources(&self) -> impl Iterator<Item = (&str, DefineSourceRole, &str)> {
        let mut entries: Vec<_> = self.files.iter().collect();
        entries.sort_by_key(|(path, _)| *path);
        entries
            .into_iter()
            .map(|(path, entry)| (path.as_str(), entry.role, entry.src.as_str()))
    }
}

fn is_inline_source(path: &str, existing_src: &str, src: &str) -> bool {
    path.starts_with("<inline:") && path.ends_with('>') && existing_src == src
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
