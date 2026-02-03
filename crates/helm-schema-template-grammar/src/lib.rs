#![allow(warnings)]

pub mod yaml {
    unsafe extern "C" {
        fn tree_sitter_yaml() -> *const ();
    }

    pub fn language() -> tree_sitter_language::LanguageFn {
        unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_yaml) }
    }

    /// The content of the [`node-types.json`][] file for the yaml grammar.
    ///
    /// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers#static-node-types
    pub const NODE_TYPES: &'static str = include_str!(concat!(
        env!("OUT_DIR"),
        "/vendor/tree-sitter-helm-template/src/node-types.json"
    ));

    #[cfg(test)]
    mod tests {
        #[test]
        fn loads_grammar() {
            let mut parser = tree_sitter::Parser::new();
            let language = tree_sitter::Language::new(super::language());
            parser.set_language(&language).unwrap();
        }
    }
}

pub mod go_template {
    unsafe extern "C" {
        fn tree_sitter_gotmpl() -> *const ();
    }

    pub fn language() -> tree_sitter_language::LanguageFn {
        unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_gotmpl) }
    }

    /// The content of the [`node-types.json`][] file for the go template grammar.
    ///
    /// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers#static-node-types
    pub const NODE_TYPES: &'static str = include_str!(concat!(
        env!("OUT_DIR"),
        "/vendor/tree-sitter-go-template/src/node-types.json"
    ));

    #[cfg(test)]
    mod tests {
        #[test]
        fn loads_grammar() {
            let mut parser = tree_sitter::Parser::new();
            let language = tree_sitter::Language::new(super::language());
            parser.set_language(&language).unwrap();
        }
    }
}
