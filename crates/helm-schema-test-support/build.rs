//! Embeds the provenance of the tree this crate is compiled from.
//!
//! `HELM_SCHEMA_GENERATION_SOURCE_SHA256` is the digest of every generation
//! input at compile time. The producer refuses to run, and consumers refuse its
//! manifest, when that digest differs from the tree they read at run time.

#[path = "src/source_digest.rs"]
mod source_digest;

use std::io;
use std::path::PathBuf;

fn main() -> io::Result<()> {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("CARGO_MANIFEST_DIR is not set"))?;
    let root = manifest_dir.join("../..");
    for input in source_digest::generation_inputs(&root) {
        println!("cargo:rerun-if-changed={}", input.display());
    }
    let digest = source_digest::generation_source_digest(&root)?;
    println!("cargo:rustc-env=HELM_SCHEMA_GENERATION_SOURCE_SHA256={digest}");
    // The `jsonschema` release the cell_matrix prober compiles schemas with, from the lockfile.
    let lockfile = root.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lockfile.display());
    let lock = std::fs::read_to_string(&lockfile)?;
    // Cargo.lock is generated: every package is a `[[package]]` line, then `name = "…"`, then
    // `version = "…"`.
    let mut in_jsonschema = false;
    let mut version = None;
    for line in lock.lines() {
        if line == "[[package]]" {
            in_jsonschema = false;
        } else if line == r#"name = "jsonschema""# {
            in_jsonschema = true;
        } else if in_jsonschema && let Some(value) = line.strip_prefix("version = ") {
            version = Some(value.trim_matches('"').to_string());
            break;
        }
    }
    let version =
        version.ok_or_else(|| io::Error::other("Cargo.lock names no jsonschema package"))?;
    println!("cargo:rustc-env=HELM_SCHEMA_JSONSCHEMA_VERSION={version}");
    for (name, variable) in [
        ("HELM_SCHEMA_BUILD_TARGET", "TARGET"),
        ("HELM_SCHEMA_BUILD_PROFILE", "PROFILE"),
    ] {
        let value = std::env::var(variable).map_err(io::Error::other)?;
        println!("cargo:rustc-env={name}={value}");
    }
    Ok(())
}
