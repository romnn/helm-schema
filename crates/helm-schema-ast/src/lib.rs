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
    pub fn get(&self, name: &str) -> Option<&[HelmAst]> {
        self.defines.get(name).map(|v| v.as_slice())
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
