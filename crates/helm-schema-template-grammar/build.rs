//! Builds the vendored tree-sitter grammars used by the template parser.

use std::path::PathBuf;

struct Grammar {
    name: &'static str,
    dir: &'static str,
    c_files: &'static [&'static str],
}

fn main() -> Result<(), std::env::VarError> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);

    let grammars = [
        Grammar {
            name: "tree_sitter_yaml",
            dir: "../../grammars/tree-sitter-yaml",
            c_files: &["src/parser.c", "src/scanner.c"],
        },
        Grammar {
            name: "tree_sitter_go_template",
            dir: "../../grammars/tree-sitter-go-template",
            c_files: &["src/parser.c"],
        },
        Grammar {
            name: "tree_sitter_helm_template",
            dir: "../../grammars/tree-sitter-helm-template",
            c_files: &["src/parser.c", "src/scanner.c"],
        },
    ];

    for g in &grammars {
        let gdir = manifest_dir.join(g.dir);
        let mut build = cc::Build::new();
        build
            .include(gdir.join("src"))
            .flag_if_supported("-w")
            .flag_if_supported("-Wno-unused-parameter")
            .flag_if_supported("-Wno-unused-but-set-variable")
            .flag_if_supported("-Wno-trigraphs");
        for c in g.c_files {
            let p = gdir.join(c);
            if p.exists() {
                build.file(p);
            }
        }
        build.compile(g.name);

        // Track the whole generated-source dir, not just the .c files: the
        // parsers also depend on the vendored headers (e.g. tree_sitter/array.h).
        println!("cargo:rerun-if-changed={}", gdir.join("src").display());
    }
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
