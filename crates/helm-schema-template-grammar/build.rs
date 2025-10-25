use git2::{Oid, Repository};
use std::path::PathBuf;

struct Grammar {
    name: &'static str,
    repo: &'static str,
    rev: &'static str,
    c_files: &'static [&'static str],
    cxx_files: &'static [&'static str],
}

fn main() {
    let grammars = [
        Grammar {
            name: "tree_sitter_yaml",
            repo: "https://github.com/tree-sitter-grammars/tree-sitter-yaml.git",
            rev: "7708026449bed86239b1cd5bce6e3c34dbca6415",
            c_files: &["src/parser.c", "src/scanner.c"],
            cxx_files: &[],
        },
        Grammar {
            name: "tree_sitter_go_template",
            repo: "https://github.com/ngalaiko/tree-sitter-go-template.git",
            rev: "ca26229bafcd3f37698a2496c2a5efa2f07e86bc",
            c_files: &["src/parser.c"],
            cxx_files: &[],
        },
    ];

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let vendor_dir = out_dir.join("vendor");
    std::fs::create_dir_all(&vendor_dir).unwrap();

    for g in &grammars {
        let gdir = vendor_dir.join(g.name);
        if !gdir.exists() {
            let repo = Repository::clone(g.repo, &gdir).expect("clone grammar");
            let oid = Oid::from_str(g.rev).expect("oid");
            let obj = repo.find_object(oid, None).unwrap();
            repo.reset(&obj, git2::ResetType::Hard, None).unwrap();
        } else {
            // ensure we are on the pinned commit
            let repo = Repository::open(&gdir).unwrap();
            repo.set_head_detached(Oid::from_str(g.rev).unwrap())
                .unwrap();
            repo.checkout_head(None).unwrap();
        }

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
    }
}
