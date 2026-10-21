//! Every git-tracked path must check out on Windows.
//!
//! A path Windows cannot create fails `git checkout` itself, so every
//! windows-latest CI job dies before a single test runs, and nothing on
//! Linux or macOS notices. This check runs in the default unit profile so
//! the failure surfaces on every platform, before the push.

use std::collections::BTreeMap;
use std::process::Command;

use color_eyre::eyre::{self, WrapErr as _};

use crate::sim_assert_eq;

/// Characters Win32 forbids anywhere in a file name. `/` never reaches a
/// component because git separates on it; `\` does, and Windows would read
/// it as a separator.
const FORBIDDEN_CHARACTERS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Device names Windows reserves regardless of case or extension.
const RESERVED_DEVICE_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com0", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
    "com8", "com9", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Longest tracked path accepted, in bytes. Git for Windows leaves
/// `core.longpaths` off, so a full path past 259 characters fails checkout;
/// this keeps room for a CI workspace prefix such as
/// `D:\a\helm-schema\helm-schema\`.
const MAX_TRACKED_PATH_BYTES: usize = 200;

/// Why Windows cannot create a file or directory named `component`, or
/// `None` when it can.
fn component_problem(component: &str) -> Option<String> {
    if let Some(character) = component
        .chars()
        .find(|character| FORBIDDEN_CHARACTERS.contains(character) || character.is_control())
    {
        return Some(format!("component {component:?} contains {character:?}"));
    }
    if component.ends_with('.') || component.ends_with(' ') {
        return Some(format!("component {component:?} ends with a dot or space"));
    }
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches(' ')
        .to_ascii_lowercase();
    if RESERVED_DEVICE_NAMES.contains(&stem.as_str()) {
        return Some(format!(
            "component {component:?} is the reserved device name {stem:?}"
        ));
    }
    None
}

/// Every reason the tracked `paths` cannot all check out on Windows, in a
/// stable order.
fn windows_checkout_problems(paths: &[String]) -> Vec<String> {
    let mut problems = Vec::new();
    // Lowercased path -> its distinct spellings, and whether any is a file.
    let mut spellings: BTreeMap<String, (Vec<String>, bool)> = BTreeMap::new();
    for path in paths {
        if path.len() > MAX_TRACKED_PATH_BYTES {
            problems.push(format!(
                "{path:?}: {} bytes, over the {MAX_TRACKED_PATH_BYTES}-byte limit",
                path.len()
            ));
        }
        for component in path.split('/') {
            if let Some(problem) = component_problem(component) {
                problems.push(format!("{path:?}: {problem}"));
            }
        }
        let mut prefix = String::new();
        let mut components = path.split('/').peekable();
        while let Some(component) = components.next() {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            let is_file = components.peek().is_none();
            let (names, any_file) = spellings.entry(prefix.to_lowercase()).or_default();
            if !names.contains(&prefix) {
                names.push(prefix.clone());
            }
            *any_file |= is_file;
        }
    }
    for (names, any_file) in spellings.values() {
        // Directories differing only in case merge harmlessly; a file
        // colliding with any other spelling overwrites or blocks it.
        if *any_file && names.len() > 1 {
            problems.push(format!("case-insensitive collision between {names:?}"));
        }
    }
    problems
}

fn tracked_paths() -> eyre::Result<Vec<String>> {
    let root = crate::workspace_root();
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "-z"])
        .output()
        .wrap_err_with(|| format!("run git ls-files in {}", root.display()))?;
    eyre::ensure!(
        output.status.success(),
        "git ls-files failed in {}: {}",
        root.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let listing = String::from_utf8(output.stdout).wrap_err("git ls-files output is not UTF-8")?;
    let paths = listing
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    eyre::ensure!(!paths.is_empty(), "git ls-files listed no paths");
    Ok(paths)
}

#[test]
fn every_tracked_path_checks_out_on_windows() -> eyre::Result<()> {
    let problems = windows_checkout_problems(&tracked_paths()?);

    sim_assert_eq!(have: problems, want: Vec::<String>::new());
    Ok(())
}

#[test]
fn checker_rejects_every_windows_checkout_failure() {
    let paths = [
        r#"testdata/provider-bundle/kubernetes-json-schema-cache/default/v1.29.0-standalone-strict/certificate-".json.not-found"#,
        "a/b<c",
        "a/b\\c",
        "a/tab\there",
        "a/trailing-dot.",
        "a/trailing-space ",
        "a/NUL.json",
        "a/com1",
        "a/Readme.md",
        "a/readme.md",
        "b/Dir/file",
        "b/dir",
    ]
    .map(str::to_string)
    .into_iter()
    .chain([format!("c/{}", "x".repeat(MAX_TRACKED_PATH_BYTES - 1))])
    .collect::<Vec<_>>();

    sim_assert_eq!(
        have: windows_checkout_problems(&paths),
        want: vec![
            r#""testdata/provider-bundle/kubernetes-json-schema-cache/default/v1.29.0-standalone-strict/certificate-\".json.not-found": component "certificate-\".json.not-found" contains '"'"#.to_string(),
            r#""a/b<c": component "b<c" contains '<'"#.to_string(),
            r#""a/b\\c": component "b\\c" contains '\\'"#.to_string(),
            r#""a/tab\there": component "tab\there" contains '\t'"#.to_string(),
            r#""a/trailing-dot.": component "trailing-dot." ends with a dot or space"#.to_string(),
            r#""a/trailing-space ": component "trailing-space " ends with a dot or space"#.to_string(),
            r#""a/NUL.json": component "NUL.json" is the reserved device name "nul""#.to_string(),
            r#""a/com1": component "com1" is the reserved device name "com1""#.to_string(),
            format!(
                "\"c/{}\": 201 bytes, over the 200-byte limit",
                "x".repeat(MAX_TRACKED_PATH_BYTES - 1)
            ),
            r#"case-insensitive collision between ["a/Readme.md", "a/readme.md"]"#.to_string(),
            r#"case-insensitive collision between ["b/Dir", "b/dir"]"#.to_string(),
        ]
    );
}

#[test]
fn checker_accepts_portable_paths() {
    let paths = [
        "testdata/provider-bundle/kubernetes-json-schema-cache/default/v1.29.0-standalone-strict/certificate-cert-manager-v1.json.not-found",
        "crates/con-figure/src/lib.rs",
        "a/Dir/x",
        "a/dir/y",
    ]
    .map(str::to_string);

    sim_assert_eq!(have: windows_checkout_problems(&paths), want: Vec::<String>::new());
}
