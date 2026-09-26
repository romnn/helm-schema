//! The digest of every workspace file that can change what generation computes.
//!
//! The build script includes this module verbatim to embed the digest of the
//! tree the crate is compiled from; the library recomputes it at run time. A
//! producer or consumer whose two values differ was built from another tree.

use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

/// Workspace crates linked into generation: the production pipeline, its
/// grammar, `test-util`, and this crate.
pub const GENERATION_CRATES: &[&str] = &[
    "helm-schema",
    "helm-schema-ast",
    "helm-schema-core",
    "helm-schema-gen",
    "helm-schema-ir",
    "helm-schema-json-schema-minify",
    "helm-schema-json-schema-walk",
    "helm-schema-k8s",
    "helm-schema-syntax",
    "helm-schema-template-grammar",
    "helm-schema-test-support",
    "test-util",
];

/// Workspace-level build inputs: manifests, the lockfile, and the Cargo
/// configuration that sets `CARGO_WORKSPACE_DIR`.
const BUILD_FILES: &[&str] = &["Cargo.toml", "Cargo.lock", ".cargo/config.toml"];

/// Vendored sources a build script compiles, relative to the workspace root.
const BUILD_SOURCE_DIRS: &[&str] =
    &["crates/helm-schema-template-grammar/grammars/tree-sitter-go-template/src"];

/// Every file or directory whose content feeds generation.
///
/// Each generation crate contributes its `Cargo.toml`, its `build.rs` when it
/// has one, and its complete `src/` directory; `src/tests/` is removed later.
#[must_use]
pub fn generation_inputs(root: &Path) -> Vec<PathBuf> {
    let mut inputs = BUILD_FILES
        .iter()
        .chain(BUILD_SOURCE_DIRS)
        .map(|path| root.join(path))
        .collect::<Vec<_>>();
    for name in GENERATION_CRATES {
        let dir = root.join("crates").join(name);
        inputs.push(dir.join("Cargo.toml"));
        if dir.join("build.rs").is_file() {
            inputs.push(dir.join("build.rs"));
        }
        inputs.push(dir.join("src"));
    }
    inputs
}

/// Digest of [`generation_inputs`], excluding everything below a `tests`
/// directory.
///
/// # Errors
///
/// Returns an error when an input cannot be read.
pub fn generation_source_digest(root: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    for input in generation_inputs(root) {
        if input.is_dir() {
            collect_files(&input, &mut files, &|path| {
                let relative = path.strip_prefix(root).unwrap_or(path);
                !relative
                    .components()
                    .any(|component| component.as_os_str() == "tests")
            })?;
        } else {
            files.push(input);
        }
    }
    digest_files(root, &files)
}

/// Appends every regular file below `dir` accepted by `keep`, skipping
/// Finder metadata that macOS may create at any time.
///
/// # Errors
///
/// Returns an error when a directory cannot be listed.
pub fn collect_files(
    dir: &Path,
    files: &mut Vec<PathBuf>,
    keep: &dyn Fn(&Path) -> bool,
) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.file_name().is_some_and(|name| name == ".DS_Store") {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, files, keep)?;
        } else if keep(&path) {
            files.push(path);
        }
    }
    Ok(())
}

/// Hashes `files` as sorted `(path relative to root, content hash)` lines.
///
/// # Errors
///
/// Returns an error naming the first file that cannot be read.
pub fn digest_files(root: &Path, files: &[PathBuf]) -> io::Result<String> {
    let mut lines = Vec::with_capacity(files.len());
    for file in files {
        let bytes = std::fs::read(file).map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", file.display()))
        })?;
        let relative = file.strip_prefix(root).unwrap_or(file);
        lines.push(format!(
            "{}\t{}\n",
            relative.to_string_lossy().replace('\\', "/"),
            sha256_hex(&bytes)
        ));
    }
    lines.sort();
    lines.dedup();
    Ok(sha256_hex(lines.concat().as_bytes()))
}

/// Lowercase hexadecimal SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        for nibble in [byte >> 4, byte & 0x0f] {
            let digit = if nibble < 10 {
                b'0' + nibble
            } else {
                b'a' + nibble - 10
            };
            hex.push(char::from(digit));
        }
    }
    hex
}
