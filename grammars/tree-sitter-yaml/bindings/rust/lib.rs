//! This crate provides YAML language support for the [tree-sitter] parsing library.
//!
//! Typically, you will use the [`LANGUAGE`] constant to add this language to a
//! tree-sitter [`Parser`], and then use the parser to parse some code:
//!
//! ```
//! let code = r#"
//! key: value
//! list:
//!     - item1
//!     - item2
//! "#;
//! let mut parser = tree_sitter::Parser::new();
//! let language = tree_sitter_yaml::LANGUAGE;
//! parser.set_language(&language.into())?;
//! let tree = parser
//!     .parse(code, None)
//!     .ok_or_else(|| std::io::Error::other("parser returned no tree"))?;
//! assert!(!tree.root_node().has_error());
//! # Ok::<_, Box<dyn std::error::Error>>(())
//! ```
//!
//! [`Parser`]: https://docs.rs/tree-sitter/latest/tree_sitter/struct.Parser.html
//! [tree-sitter]: https://tree-sitter.github.io/

use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_yaml() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = {
    // SAFETY: `parser.c` exports this symbol with tree-sitter's language-function ABI.
    unsafe { LanguageFn::from_raw(tree_sitter_yaml) }
};

/// The content of the [`node-types.json`] file for this grammar.
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers/6-static-node-types
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

/// The highlight queries for this grammar.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

#[cfg(test)]
mod tests {
    #[test]
    fn test_can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading YAML parser");
    }
}
