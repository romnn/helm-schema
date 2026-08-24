//! Builds the vendored tree-sitter Go-template grammar.

use std::path::PathBuf;

fn main() -> Result<(), std::env::VarError> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let grammar_dir = manifest_dir.join("grammars/tree-sitter-go-template");
    cc::Build::new()
        .include(grammar_dir.join("src"))
        .flag_if_supported("-w")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-trigraphs")
        .file(grammar_dir.join("src/parser.c"))
        .compile("tree_sitter_go_template");

    // Track the complete generated-source directory because the parser also
    // depends on vendored tree-sitter headers.
    println!(
        "cargo:rerun-if-changed={}",
        grammar_dir.join("src").display()
    );
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
