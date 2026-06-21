pub mod expr;
mod tree_sitter_parser;
mod values_comments;

pub use expr::{Literal, TemplateExpr, parse_action_expressions};
pub use tree_sitter_parser::{ParsedTemplate, TreeSitterParser, contains_template_action};
pub use values_comments::extract_values_yaml_descriptions;

use std::collections::{HashMap, hash_map::Entry};
use std::fmt::Write;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("tree-sitter parse failed")]
    TreeSitterParseFailed,
}

/// Shared AST for Helm+YAML templates.
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
    #[must_use]
    pub fn new(raw: impl Into<String>, expr: TemplateExpr) -> Self {
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
    pub fn parse_range(raw: impl Into<String>) -> Self {
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

/// Parsed Helm action/output node.
///
/// Like [`TemplateHeader`], this preserves the original unwrapped action text
/// while also storing the typed top-level expressions produced by the action.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateAction {
    raw: String,
    exprs: Vec<TemplateExpr>,
}

impl TemplateAction {
    #[must_use]
    pub fn new(raw: impl Into<String>, exprs: Vec<TemplateExpr>) -> Self {
        Self {
            raw: raw.into(),
            exprs,
        }
    }

    #[must_use]
    pub fn parse(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let wrapped = format!("{{{{ {raw} }}}}");
        let exprs = parse_action_expressions(&wrapped);
        Self::new(raw, exprs)
    }

    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub fn exprs(&self) -> &[TemplateExpr] {
        &self.exprs
    }

    #[must_use]
    pub fn renders_yaml_fragment(&self) -> bool {
        self.exprs.iter().any(TemplateExpr::renders_yaml_fragment)
    }

    #[must_use]
    pub fn may_inject_yaml_structure(&self) -> bool {
        self.exprs
            .iter()
            .any(TemplateExpr::may_inject_yaml_structure)
    }

    #[must_use]
    pub fn fragment_indent_width(&self) -> Option<usize> {
        self.exprs
            .iter()
            .rev()
            .find_map(TemplateExpr::fragment_indent_width)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HelmAst {
    Document {
        items: Vec<HelmAst>,
    },

    Mapping {
        items: Vec<HelmAst>,
    },
    Pair {
        key: Box<HelmAst>,
        value: Option<Box<HelmAst>>,
    },

    Sequence {
        items: Vec<HelmAst>,
    },

    Scalar {
        text: String,
    },

    HelmExpr {
        action: TemplateAction,
    },
    HelmComment {
        text: String,
    },

    If {
        condition: TemplateHeader,
        then_branch: Vec<HelmAst>,
        else_branch: Vec<HelmAst>,
    },
    Range {
        header: TemplateHeader,
        body: Vec<HelmAst>,
        else_branch: Vec<HelmAst>,
    },
    With {
        header: TemplateHeader,
        body: Vec<HelmAst>,
        else_branch: Vec<HelmAst>,
    },
    Define {
        name: String,
        body: Vec<HelmAst>,
    },
    Block {
        name: String,
        body: Vec<HelmAst>,
    },
}

impl HelmAst {
    /// Render this AST as a pretty-printed S-expression string.
    #[must_use]
    pub fn to_sexpr(&self) -> String {
        let mut buf = String::new();
        self.write_sexpr(&mut buf, 0);
        buf
    }

    /// Visit every top-level template expression structurally embedded in this
    /// AST node: standalone Helm actions plus control-flow headers.
    pub fn walk_template_expr_roots(&self, visit: &mut impl FnMut(&TemplateExpr)) {
        match self {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Sequence { items } => {
                for item in items {
                    item.walk_template_expr_roots(visit);
                }
            }
            HelmAst::Pair { key, value } => {
                key.walk_template_expr_roots(visit);
                if let Some(value) = value.as_deref() {
                    value.walk_template_expr_roots(visit);
                }
            }
            HelmAst::HelmExpr { action } => {
                for expr in action.exprs() {
                    visit(expr);
                }
            }
            HelmAst::If {
                condition,
                then_branch,
                else_branch,
            } => {
                visit(condition.expr());
                for item in then_branch {
                    item.walk_template_expr_roots(visit);
                }
                for item in else_branch {
                    item.walk_template_expr_roots(visit);
                }
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            }
            | HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                visit(header.expr());
                for item in body {
                    item.walk_template_expr_roots(visit);
                }
                for item in else_branch {
                    item.walk_template_expr_roots(visit);
                }
            }
            HelmAst::Define { body, .. } | HelmAst::Block { body, .. } => {
                for item in body {
                    item.walk_template_expr_roots(visit);
                }
            }
            HelmAst::Scalar { .. } | HelmAst::HelmComment { .. } => {}
        }
    }

    /// Visit every typed template expression reachable from this AST node,
    /// including actions embedded inside scalar text fragments.
    pub fn walk_template_exprs(&self, visit: &mut impl FnMut(&TemplateExpr)) {
        match self {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Sequence { items } => {
                for item in items {
                    item.walk_template_exprs(visit);
                }
            }
            HelmAst::Pair { key, value } => {
                key.walk_template_exprs(visit);
                if let Some(value) = value.as_deref() {
                    value.walk_template_exprs(visit);
                }
            }
            HelmAst::Scalar { text } => {
                for expr in parse_action_expressions(text) {
                    visit(&expr);
                }
            }
            HelmAst::HelmExpr { action } => {
                for expr in action.exprs() {
                    visit(expr);
                }
            }
            HelmAst::If {
                condition,
                then_branch,
                else_branch,
            } => {
                visit(condition.expr());
                for item in then_branch {
                    item.walk_template_exprs(visit);
                }
                for item in else_branch {
                    item.walk_template_exprs(visit);
                }
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            }
            | HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                visit(header.expr());
                for item in body {
                    item.walk_template_exprs(visit);
                }
                for item in else_branch {
                    item.walk_template_exprs(visit);
                }
            }
            HelmAst::Define { body, .. } | HelmAst::Block { body, .. } => {
                for item in body {
                    item.walk_template_exprs(visit);
                }
            }
            HelmAst::HelmComment { .. } => {}
        }
    }

    #[allow(clippy::too_many_lines)]
    fn write_sexpr(&self, buf: &mut String, indent: usize) {
        let pad = "  ".repeat(indent);
        match self {
            HelmAst::Document { items } => {
                let _ = write!(buf, "{pad}(Document");
                for item in items {
                    buf.push('\n');
                    item.write_sexpr(buf, indent + 1);
                }
                buf.push(')');
            }
            HelmAst::Mapping { items } => {
                let _ = write!(buf, "{pad}(Mapping");
                for item in items {
                    buf.push('\n');
                    item.write_sexpr(buf, indent + 1);
                }
                buf.push(')');
            }
            HelmAst::Pair { key, value } => {
                let _ = write!(buf, "{pad}(Pair");
                buf.push('\n');
                key.write_sexpr(buf, indent + 1);
                if let Some(v) = value {
                    buf.push('\n');
                    v.write_sexpr(buf, indent + 1);
                }
                buf.push(')');
            }
            HelmAst::Sequence { items } => {
                let _ = write!(buf, "{pad}(Sequence");
                for item in items {
                    buf.push('\n');
                    item.write_sexpr(buf, indent + 1);
                }
                buf.push(')');
            }
            HelmAst::Scalar { text } => {
                let _ = write!(buf, "{pad}(Scalar {text:?})");
            }
            HelmAst::HelmExpr { action } => {
                let _ = write!(buf, "{pad}(HelmExpr {:?})", action.raw());
            }
            HelmAst::HelmComment { text } => {
                let _ = write!(buf, "{pad}(HelmComment {text:?})");
            }
            HelmAst::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let _ = write!(buf, "{pad}(If {:?}", condition.raw(),);
                if !then_branch.is_empty() {
                    let _ = write!(buf, "\n{pad}  (then");
                    for item in then_branch {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                if !else_branch.is_empty() {
                    let _ = write!(buf, "\n{pad}  (else");
                    for item in else_branch {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                buf.push(')');
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            } => {
                let _ = write!(buf, "{pad}(Range {:?}", header.raw(),);
                if !body.is_empty() {
                    let _ = write!(buf, "\n{pad}  (body");
                    for item in body {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                if !else_branch.is_empty() {
                    let _ = write!(buf, "\n{pad}  (else");
                    for item in else_branch {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                buf.push(')');
            }
            HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                let _ = write!(buf, "{pad}(With {:?}", header.raw(),);
                if !body.is_empty() {
                    let _ = write!(buf, "\n{pad}  (body");
                    for item in body {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                if !else_branch.is_empty() {
                    let _ = write!(buf, "\n{pad}  (else");
                    for item in else_branch {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                buf.push(')');
            }
            HelmAst::Define { name, body } => {
                let _ = write!(buf, "{pad}(Define {name:?}");
                if !body.is_empty() {
                    for item in body {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 1);
                    }
                }
                buf.push(')');
            }
            HelmAst::Block { name, body } => {
                let _ = write!(buf, "{pad}(Block {name:?}");
                if !body.is_empty() {
                    for item in body {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 1);
                    }
                }
                buf.push(')');
            }
        }
    }
}

/// Trait for parsing Helm+YAML templates into a shared [`HelmAst`].
pub trait HelmParser {
    /// Parse Helm+YAML template source into a [`HelmAst`].
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if the input cannot be parsed.
    fn parse(&self, src: &str) -> Result<HelmAst, ParseError>;
}

/// Index of named template definitions (`{{ define "name" }}...{{ end }}`).
///
/// Populated by feeding helper files through [`DefineIndex::add_source`].
#[derive(Default, Debug, Clone)]
pub struct DefineIndex {
    defines: HashMap<String, Vec<HelmAst>>,
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

    /// Parse `src` with `parser` and collect all `Define` blocks into the index.
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if `parser` fails to parse `src`.
    pub fn add_source(&mut self, parser: &dyn HelmParser, src: &str) -> Result<(), ParseError> {
        let tree = parser.parse(src)?;
        self.collect_defines(&tree);
        if !self.files.values().any(|existing| existing == src) {
            let mut index = self.files.len();
            loop {
                let path = format!("<inline:{index}>");
                if let Entry::Vacant(entry) = self.files.entry(path) {
                    entry.insert(src.to_string());
                    break;
                }
                index += 1;
            }
        }
        Ok(())
    }

    /// Look up a named template definition.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[HelmAst]> {
        self.defines.get(name).map(std::vec::Vec::as_slice)
    }

    fn collect_defines(&mut self, node: &HelmAst) {
        match node {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Sequence { items } => {
                for item in items {
                    self.collect_defines(item);
                }
            }
            HelmAst::Define { name, body } => {
                self.defines.insert(name.clone(), body.clone());
            }
            HelmAst::If {
                then_branch,
                else_branch,
                ..
            } => {
                for item in then_branch {
                    self.collect_defines(item);
                }
                for item in else_branch {
                    self.collect_defines(item);
                }
            }
            HelmAst::Range {
                body, else_branch, ..
            }
            | HelmAst::With {
                body, else_branch, ..
            } => {
                for item in body {
                    self.collect_defines(item);
                }
                for item in else_branch {
                    self.collect_defines(item);
                }
            }
            HelmAst::Block { body, .. } => {
                for item in body {
                    self.collect_defines(item);
                }
            }
            HelmAst::Pair { value, .. } => {
                if let Some(v) = value {
                    self.collect_defines(v);
                }
            }
            HelmAst::Scalar { .. } | HelmAst::HelmExpr { .. } | HelmAst::HelmComment { .. } => {}
        }
    }
}

fn is_inline_source(path: &str, existing_src: &str, src: &str) -> bool {
    path.starts_with("<inline:") && path.ends_with('>') && existing_src == src
}

#[cfg(test)]
mod tests;
