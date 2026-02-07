pub mod yaml {
    unsafe extern "C" {
        fn tree_sitter_yaml() -> *const ();
    }

    pub fn language() -> tree_sitter_language::LanguageFn {
        unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_yaml) }
    }
}

pub mod go_template {
    unsafe extern "C" {
        fn tree_sitter_gotmpl() -> *const ();
    }

    pub fn language() -> tree_sitter_language::LanguageFn {
        unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_gotmpl) }
    }
}
