mod fused_rust;
mod tree_sitter_parser;

pub use fused_rust::FusedRustParser;
pub use tree_sitter_parser::TreeSitterParser;

use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("fused parser error: {0}")]
    FusedParse(#[from] yaml_rust::fused::FusedParseError),

    #[error("tree-sitter parse failed")]
    TreeSitterParseFailed,
}

/// Shared AST for fused Helm+YAML templates.
///
/// Both the pure-Rust yaml-rust parser and the tree-sitter fused grammar
/// produce this same representation.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        text: String,
    },
    HelmComment {
        text: String,
    },

    If {
        cond: String,
        then_branch: Vec<HelmAst>,
        else_branch: Vec<HelmAst>,
    },
    Range {
        header: String,
        body: Vec<HelmAst>,
        else_branch: Vec<HelmAst>,
    },
    With {
        header: String,
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

    fn write_sexpr(&self, buf: &mut String, indent: usize) {
        let pad = "  ".repeat(indent);
        match self {
            HelmAst::Document { items } => {
                buf.push_str(&format!("{pad}(Document"));
                for item in items {
                    buf.push('\n');
                    item.write_sexpr(buf, indent + 1);
                }
                buf.push(')');
            }
            HelmAst::Mapping { items } => {
                buf.push_str(&format!("{pad}(Mapping"));
                for item in items {
                    buf.push('\n');
                    item.write_sexpr(buf, indent + 1);
                }
                buf.push(')');
            }
            HelmAst::Pair { key, value } => {
                buf.push_str(&format!("{pad}(Pair"));
                buf.push('\n');
                key.write_sexpr(buf, indent + 1);
                if let Some(v) = value {
                    buf.push('\n');
                    v.write_sexpr(buf, indent + 1);
                }
                buf.push(')');
            }
            HelmAst::Sequence { items } => {
                buf.push_str(&format!("{pad}(Sequence"));
                for item in items {
                    buf.push('\n');
                    item.write_sexpr(buf, indent + 1);
                }
                buf.push(')');
            }
            HelmAst::Scalar { text } => {
                buf.push_str(&format!("{pad}(Scalar {text:?})"));
            }
            HelmAst::HelmExpr { text } => {
                buf.push_str(&format!("{pad}(HelmExpr {text:?})"));
            }
            HelmAst::HelmComment { text } => {
                buf.push_str(&format!("{pad}(HelmComment {text:?})"));
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                buf.push_str(&format!("{pad}(If {cond:?}"));
                if !then_branch.is_empty() {
                    buf.push_str(&format!("\n{pad}  (then"));
                    for item in then_branch {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                if !else_branch.is_empty() {
                    buf.push_str(&format!("\n{pad}  (else"));
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
                buf.push_str(&format!("{pad}(Range {header:?}"));
                if !body.is_empty() {
                    buf.push_str(&format!("\n{pad}  (body"));
                    for item in body {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                if !else_branch.is_empty() {
                    buf.push_str(&format!("\n{pad}  (else"));
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
                buf.push_str(&format!("{pad}(With {header:?}"));
                if !body.is_empty() {
                    buf.push_str(&format!("\n{pad}  (body"));
                    for item in body {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                if !else_branch.is_empty() {
                    buf.push_str(&format!("\n{pad}  (else"));
                    for item in else_branch {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 2);
                    }
                    buf.push(')');
                }
                buf.push(')');
            }
            HelmAst::Define { name, body } => {
                buf.push_str(&format!("{pad}(Define {name:?}"));
                if !body.is_empty() {
                    for item in body {
                        buf.push('\n');
                        item.write_sexpr(buf, indent + 1);
                    }
                }
                buf.push(')');
            }
            HelmAst::Block { name, body } => {
                buf.push_str(&format!("{pad}(Block {name:?}"));
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
    fn parse(&self, src: &str) -> Result<HelmAst, ParseError>;
}

/// Index of named template definitions (`{{ define "name" }}...{{ end }}`).
///
/// Populated by feeding helper files through [`DefineIndex::add_source`].
#[derive(Default, Debug, Clone)]
pub struct DefineIndex {
    defines: HashMap<String, Vec<HelmAst>>,
}

impl DefineIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse `src` with `parser` and collect all `Define` blocks into the index.
    pub fn add_source(&mut self, parser: &dyn HelmParser, src: &str) -> Result<(), ParseError> {
        let tree = parser.parse(src)?;
        self.collect_defines(&tree);
        Ok(())
    }

    /// Look up a named template definition.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[HelmAst]> {
        self.defines.get(name).map(std::vec::Vec::as_slice)
    }

    fn collect_defines(&mut self, node: &HelmAst) {
        match node {
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
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
            HelmAst::Sequence { items } => {
                for item in items {
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

#[cfg(test)]
mod tests;
