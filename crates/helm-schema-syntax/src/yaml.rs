//! YAML concrete syntax trees of rendered skeletons, parsed by the
//! tree-sitter YAML grammar.
//!
//! The grammar resolves plain scalars with the YAML 1.2 core schema, while
//! Helm reads YAML 1.1: `y`, `yes` and `on` are booleans and `010` is octal
//! there. The tree therefore supplies structure and scalar style only; a
//! plain scalar's type or value never comes from the grammar's scalar kinds.

thread_local! {
    /// A `Parser` is far more expensive to build than to reuse.
    static YAML_PARSER: std::cell::RefCell<Option<tree_sitter::Parser>> =
        std::cell::RefCell::new(new_yaml_parser());
}

fn new_yaml_parser() -> Option<tree_sitter::Parser> {
    let language = tree_sitter::Language::new(tree_sitter_yaml::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    Some(parser)
}

/// Parse `text` as a YAML stream. `None` when the grammar cannot be loaded
/// or the parse is cancelled; a malformed stream still yields a tree whose
/// error and missing nodes mark the malformed parts.
#[must_use]
pub fn parse_yaml(text: &str) -> Option<tree_sitter::Tree> {
    YAML_PARSER.with_borrow_mut(|parser| parser.as_mut()?.parse(text, None))
}

#[cfg(test)]
#[path = "tests/yaml.rs"]
mod tests;
