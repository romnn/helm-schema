//! This crate provides Go-template language support for the [tree-sitter] parsing library.
//!
//! Use [`LANGUAGE`] to configure a tree-sitter [`Parser`]:
//!
//! ```
//! let code = "{{ .Values.name }}";
//! let mut parser = tree_sitter::Parser::new();
//! let language = tree_sitter_go_template::LANGUAGE;
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
    fn tree_sitter_gotmpl() -> *const ();
}

/// The tree-sitter language function for this grammar.
pub const LANGUAGE: LanguageFn = {
    // SAFETY: `parser.c` exports this symbol with tree-sitter's language-function ABI.
    unsafe { LanguageFn::from_raw(tree_sitter_gotmpl) }
};

/// The content of the [`node-types.json`][] file for this grammar.
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers#static-node-types
pub const NODE_TYPES: &'static str = include_str!("../../src/node-types.json");

// Uncomment these to include any queries that this grammar contains

// pub const HIGHLIGHTS_QUERY: &'static str = include_str!("../../queries/highlights.scm");
// pub const INJECTIONS_QUERY: &'static str = include_str!("../../queries/injections.scm");
// pub const LOCALS_QUERY: &'static str = include_str!("../../queries/locals.scm");
// pub const TAGS_QUERY: &'static str = include_str!("../../queries/tags.scm");

#[cfg(test)]
mod tests {
    #[test]
    fn test_can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading go_template language");
    }
}
