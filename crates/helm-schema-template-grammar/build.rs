use std::path::PathBuf;

struct Grammar {
    name: &'static str,
    dir: &'static str,
    c_files: &'static [&'static str],
    defines: &'static [(&'static str, &'static str)],
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    let grammars = [
        Grammar {
            name: "tree_sitter_yaml",
            dir: "../../grammars/tree-sitter-yaml",
            c_files: &["src/parser.c", "src/scanner.c"],
            defines: &[],
        },
        Grammar {
            name: "tree_sitter_go_template",
            dir: "../../grammars/tree-sitter-go-template",
            c_files: &["src/parser.c"],
            defines: &[],
        },
        Grammar {
            name: "tree_sitter_helm_template",
            dir: "../../grammars/tree-sitter-helm-template",
            c_files: &["src/parser.c", "src/scanner.c"],
            // The fused grammar is unfortunately named `yaml` upstream and exports
            // `tree_sitter_yaml` + `tree_sitter_yaml_external_scanner_*`, which would
            // collide with the standalone YAML grammar we also ship.
            // Rename its symbols at compile time.
            defines: &[
                ("tree_sitter_yaml", "tree_sitter_helm_template"),
                (
                    "tree_sitter_yaml_external_scanner_create",
                    "tree_sitter_helm_template_external_scanner_create",
                ),
                (
                    "tree_sitter_yaml_external_scanner_destroy",
                    "tree_sitter_helm_template_external_scanner_destroy",
                ),
                (
                    "tree_sitter_yaml_external_scanner_serialize",
                    "tree_sitter_helm_template_external_scanner_serialize",
                ),
                (
                    "tree_sitter_yaml_external_scanner_deserialize",
                    "tree_sitter_helm_template_external_scanner_deserialize",
                ),
                (
                    "tree_sitter_yaml_external_scanner_scan",
                    "tree_sitter_helm_template_external_scanner_scan",
                ),
            ],
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
        for &(k, v) in g.defines {
            build.define(k, Some(v));
        }
        for c in g.c_files {
            let p = gdir.join(c);
            if p.exists() {
                build.file(p);
            }
        }
        build.compile(g.name);
        println!("cargo:rerun-if-changed=build.rs");

        for c in g.c_files {
            println!("cargo:rerun-if-changed={}", gdir.join(c).display());
        }
    }
}
