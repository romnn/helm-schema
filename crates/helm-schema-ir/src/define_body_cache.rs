use std::cell::RefCell;
use std::collections::HashMap;

use helm_schema_ast::DefineIndex;

pub(crate) struct DefineBodyCache {
    sources: HashMap<String, String>,
    trees: RefCell<HashMap<String, tree_sitter::Tree>>,
}

impl DefineBodyCache {
    #[tracing::instrument(skip_all)]
    pub(crate) fn new(defines: &DefineIndex) -> Self {
        Self {
            sources: collect_define_body_sources(defines),
            trees: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn source(&self, name: &str) -> Option<&str> {
        self.sources.get(name).map(String::as_str)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn tree(&self, name: &str) -> Option<tree_sitter::Tree> {
        if let Some(tree) = self.trees.borrow().get(name) {
            return Some(tree.clone());
        }

        let src = self.source(name)?;
        let tree = parse_go_template(src)?;
        self.trees
            .borrow_mut()
            .insert(name.to_string(), tree.clone());
        Some(tree)
    }
}

#[tracing::instrument(skip_all, fields(bytes = src.len()))]
pub(crate) fn parse_go_template(src: &str) -> Option<tree_sitter::Tree> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return None;
    }
    parser.parse(src, None)
}

#[tracing::instrument(skip_all)]
fn collect_define_body_sources(defines: &DefineIndex) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (_path, src) in defines.file_sources() {
        for block in crate::extract_define_blocks(src) {
            out.insert(block.name, block.body);
        }
    }
    out
}
