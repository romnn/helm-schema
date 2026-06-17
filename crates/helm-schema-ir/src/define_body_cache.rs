use std::cell::RefCell;
use std::collections::HashMap;

use helm_schema_ast::DefineIndex;

pub(crate) struct DefineBodyCache {
    sources: HashMap<String, String>,
    source_paths: HashMap<String, String>,
    body_offsets: HashMap<String, usize>,
    structured_sources: HashMap<String, String>,
    trees: RefCell<HashMap<String, tree_sitter::Tree>>,
    structured_trees: RefCell<HashMap<String, tree_sitter::Tree>>,
}

impl DefineBodyCache {
    #[tracing::instrument(skip_all)]
    pub(crate) fn new(defines: &DefineIndex) -> Self {
        let define_bodies = collect_define_body_sources(defines);
        let sources: HashMap<String, String> = define_bodies
            .iter()
            .map(|(name, body)| (name.clone(), body.source.clone()))
            .collect();
        let source_paths: HashMap<String, String> = define_bodies
            .iter()
            .map(|(name, body)| (name.clone(), body.source_path.clone()))
            .collect();
        let body_offsets: HashMap<String, usize> = define_bodies
            .iter()
            .map(|(name, body)| (name.clone(), body.body_offset))
            .collect();
        let structured_sources: HashMap<String, String> = sources
            .iter()
            .map(|(name, source)| (name.clone(), wrap_define_body_source(source)))
            .collect();
        Self {
            sources,
            source_paths,
            body_offsets,
            structured_sources,
            trees: RefCell::new(HashMap::new()),
            structured_trees: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn source(&self, name: &str) -> Option<&str> {
        self.sources.get(name).map(String::as_str)
    }

    pub(crate) fn structured_source(&self, name: &str) -> Option<&str> {
        self.structured_sources.get(name).map(String::as_str)
    }

    pub(crate) fn source_path(&self, name: &str) -> Option<&str> {
        self.source_paths.get(name).map(String::as_str)
    }

    pub(crate) fn body_offset(&self, name: &str) -> Option<usize> {
        self.body_offsets.get(name).copied()
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

    #[tracing::instrument(skip_all)]
    pub(crate) fn structured_tree(&self, name: &str) -> Option<tree_sitter::Tree> {
        if let Some(tree) = self.structured_trees.borrow().get(name) {
            return Some(tree.clone());
        }

        let src = self.structured_source(name)?;
        let tree = parse_go_template(src)?;
        self.structured_trees
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

struct CachedDefineBody {
    source: String,
    source_path: String,
    body_offset: usize,
}

#[tracing::instrument(skip_all)]
fn collect_define_body_sources(defines: &DefineIndex) -> HashMap<String, CachedDefineBody> {
    let mut out = HashMap::new();
    for (path, src) in defines.file_sources() {
        for block in crate::extract_define_blocks(src) {
            out.insert(
                block.name,
                CachedDefineBody {
                    source: block.body,
                    source_path: path.to_string(),
                    body_offset: block.body_range.start,
                },
            );
        }
    }
    out
}

fn wrap_define_body_source(source: &str) -> String {
    format!("{{{{- define \"__helm_schema_body__\" -}}}}{source}{{{{- end -}}}}")
}
