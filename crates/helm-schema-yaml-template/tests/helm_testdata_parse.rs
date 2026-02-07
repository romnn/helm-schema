use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use yaml_rust::scanner::Scanner;
use yaml_rust::YamlLoader;

fn is_yaml_template_file(path: &Path) -> bool {
    match path.extension().and_then(|s| s.to_str()) {
        Some("yaml") | Some("yml") => true,
        _ => false,
    }
}

fn is_non_yaml_template_file(path: &Path) -> bool {
    if path.file_name() == Some(OsStr::new("NOTES.txt")) {
        return true;
    }
    match path.extension().and_then(|s| s.to_str()) {
        Some("tpl") | Some("txt") => true,
        _ => false,
    }
}

fn has_templates_dir_component(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == OsStr::new("templates"))
}

fn collect_template_files(root: &Path, predicate: fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::<PathBuf>::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for ent in entries.flatten() {
            let path = ent.path();
            let ft = match ent.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };

            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            if !has_templates_dir_component(&path) {
                continue;
            }
            if !predicate(&path) {
                continue;
            }

            out.push(path);
        }
    }

    out.sort();
    out
}

#[test]
fn parse_all_testdata_helm_yaml_templates() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("helm-schema-mapper")
        .join("testdata");

    let files = collect_template_files(&root, is_yaml_template_file);
    assert!(
        !files.is_empty(),
        "no template files found under {:?}",
        root
    );

    let mut failures: Vec<String> = Vec::new();
    for p in files {
        let src = match fs::read_to_string(&p) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}: read error: {e}", p.display()));
                continue;
            }
        };

        if let Err(e) = YamlLoader::load_from_str(&src) {
            failures.push(format!("{}: {e}", p.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "failed to parse one or more templates ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn parse_representative_yaml_template_to_mapping() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("helm-schema-mapper")
        .join("testdata")
        .join("charts")
        .join("bitnami-redis")
        .join("templates")
        .join("master")
        .join("application.yaml");

    let src = fs::read_to_string(&p).expect("read representative template");

    let docs = match YamlLoader::load_from_str(&src) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("failed to parse representative template: {e}");
            eprintln!("-- token dump around line {} --", e.marker().line() + 1);

            let min_line = e.marker().line().saturating_sub(5);
            let max_line = e.marker().line() + 5;
            let mut sc = Scanner::new(src.chars());
            while let Some(tok) = sc.next() {
                let line = tok.0.line();
                if line < min_line || line > max_line {
                    continue;
                }
                eprintln!(
                    "tok @ {}:{} -> {:?}",
                    line + 1,
                    tok.0.col() + 1,
                    tok.1
                );
            }
            if let Some(se) = sc.get_error() {
                eprintln!("scanner error: {se}");
            }

            panic!("parse representative template: {}", e);
        }
    };
    assert!(!docs.is_empty(), "expected at least one YAML document");
    assert!(docs[0].as_hash().is_some(), "expected first document to be a mapping");
}

#[test]
fn parse_networkpolicy_yaml_template() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("helm-schema-mapper")
        .join("testdata")
        .join("charts")
        .join("bitnami-redis")
        .join("templates")
        .join("networkpolicy.yaml");

    let src = fs::read_to_string(&p).expect("read networkpolicy template");

    let docs = match YamlLoader::load_from_str(&src) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("failed to parse networkpolicy template: {e}");
            eprintln!("-- token dump around line {} --", e.marker().line() + 1);
            let min_line = e.marker().line().saturating_sub(5);
            let max_line = e.marker().line() + 5;
            let mut sc = Scanner::new(src.chars());
            while let Some(tok) = sc.next() {
                let line = tok.0.line();
                if line < min_line || line > max_line {
                    continue;
                }
                eprintln!(
                    "tok @ {}:{} -> {:?}",
                    line + 1,
                    tok.0.col() + 1,
                    tok.1
                );
            }
            if let Some(se) = sc.get_error() {
                eprintln!("scanner error: {se}");
            }
            panic!("parse networkpolicy template: {}", e);
        }
    };

    assert!(!docs.is_empty(), "expected at least one YAML document");
}

#[test]
fn parse_ports_configmap_yaml_template() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("helm-schema-mapper")
        .join("testdata")
        .join("charts")
        .join("bitnami-redis")
        .join("templates")
        .join("sentinel")
        .join("ports-configmap.yaml");

    let src = fs::read_to_string(&p).expect("read ports-configmap template");

    let docs = match YamlLoader::load_from_str(&src) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("failed to parse ports-configmap template: {e}");
            eprintln!("-- token dump around line {} --", e.marker().line() + 1);
            let min_line = e.marker().line().saturating_sub(10);
            let max_line = e.marker().line() + 10;
            let mut sc = Scanner::new(src.chars());
            while let Some(tok) = sc.next() {
                let line = tok.0.line();
                if line < min_line || line > max_line {
                    continue;
                }
                eprintln!(
                    "tok @ {}:{} -> {:?}",
                    line + 1,
                    tok.0.col() + 1,
                    tok.1
                );
            }
            if let Some(se) = sc.get_error() {
                eprintln!("scanner error: {se}");
            }
            panic!("parse ports-configmap template: {}", e);
        }
    };

    assert!(!docs.is_empty(), "expected at least one YAML document");
}

#[test]
fn scan_all_testdata_non_yaml_templates() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("helm-schema-mapper")
        .join("testdata");

    let files = collect_template_files(&root, is_non_yaml_template_file);
    assert!(
        !files.is_empty(),
        "no non-yaml template files found under {:?}",
        root
    );

    for p in files {
        let src = fs::read_to_string(&p).expect("read non-yaml template");
        let mut sc = Scanner::new(src.chars());
        while let Some(_tok) = sc.next() {}
        let _ = sc.get_error();
    }
}
