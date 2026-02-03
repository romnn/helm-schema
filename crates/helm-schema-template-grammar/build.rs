use std::path::PathBuf;

struct Grammar {
    name: &'static str,
    dir: &'static str,
    vendor_dir: &'static str,
    c_files: &'static [&'static str],
    cxx_files: &'static [&'static str],
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let vendor_dir = out_dir.join("vendor");
    std::fs::create_dir_all(&vendor_dir).unwrap();

    let grammars = [
        Grammar {
            name: "tree_sitter_yaml",
            dir: "../../grammars/tree-sitter-helm-template",
            vendor_dir: "tree-sitter-helm-template",
            c_files: &["src/parser.c", "src/scanner.c"],
            cxx_files: &[],
        },
        Grammar {
            name: "tree_sitter_go_template",
            dir: "../../grammars/tree-sitter-go-template",
            vendor_dir: "tree-sitter-go-template",
            c_files: &["src/parser.c"],
            cxx_files: &[],
        },
    ];

    for g in &grammars {
        let gdir = manifest_dir.join(g.dir);
        let vdir = vendor_dir.join(g.vendor_dir);
        let vsrc = vdir.join("src");
        std::fs::create_dir_all(&vsrc).unwrap();
        std::fs::copy(gdir.join("src/node-types.json"), vsrc.join("node-types.json")).unwrap();

        let mut build = cc::Build::new();
        build
            .include(gdir.join("src"))
            .flag_if_supported("-Wno-unused-parameter")
            .flag_if_supported("-Wno-unused-but-set-variable")
            .flag_if_supported("-Wno-trigraphs");
        for c in g.c_files {
            let p = gdir.join(c);
            if p.exists() {
                build.file(p);
            }
        }
        if !g.cxx_files.is_empty() {
            build.cpp(true);
            for ccxx in g.cxx_files {
                let p = gdir.join(ccxx);
                if p.exists() {
                    build.file(p);
                }
            }
        }
        build.compile(g.name);
        println!("cargo:rerun-if-changed=build.rs");

        for c in g.c_files {
            println!("cargo:rerun-if-changed={}", gdir.join(c).display());
        }
        for ccxx in g.cxx_files {
            println!("cargo:rerun-if-changed={}", gdir.join(ccxx).display());
        }
        println!("cargo:rerun-if-changed={}", gdir.join("src/node-types.json").display());
    }
}
