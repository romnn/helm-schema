//! Pin: production code in `helm-schema-ir` must NOT contain bare
//! `eprintln!` / `println!` calls. The CLI's `--diag-format=json` mode
//! contract is "every stderr line after argv parsing is a Diagnostic
//! JSON object"; a debug println anywhere on the hot path would
//! silently break that.
//!
//! Debug output must go through tracing so diagnostic-mode stderr remains
//! machine-readable.

use std::fs;
use std::path::Path;

fn walk_rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rs_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_bare_eprintln_in_helm_schema_ir_src() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files(&src_dir, &mut files);
    assert!(!files.is_empty(), "src dir is empty? {src_dir:?}");

    let mut offenders: Vec<String> = Vec::new();
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (lineno, line) in text.lines().enumerate() {
            // Skip comments — comment-only mentions are fine
            // (e.g. doc-string explaining the policy).
            let stripped = line.trim_start();
            if stripped.starts_with("//") {
                continue;
            }
            if stripped.contains("eprintln!") || stripped.contains("println!") {
                offenders.push(format!(
                    "{}:{}: {}",
                    file.display(),
                    lineno + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "production code MUST NOT contain bare eprintln!/println! calls (they violate the \
         --diag-format=json stderr contract). Use tracing::{{debug,info,…}} instead. Offenders:\n  {}",
        offenders.join("\n  ")
    );
}
