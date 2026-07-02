use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Filename of the per-root cache layout marker.
pub const LAYOUT_MARKER_FILENAME: &str = "CACHE_LAYOUT_VERSION";

/// The compiled-in cache layout version. Incrementing this constant in
/// a new helm-schema build triggers per-root invalidation for any cache
/// that records a lower (or missing) marker.
pub const CACHE_LAYOUT_VERSION: u32 = 1;

/// Compute the cache path for a single K8s schema file under the
/// canonical filesystem model: `<root>/<source_id>/<version_dir>/<filename>`.
#[must_use]
pub fn k8s_cache_path(root: &Path, source_id: &str, version_dir: &str, filename: &str) -> PathBuf {
    root.join(source_id).join(version_dir).join(filename)
}

/// Compute the cache path for a single CRD schema file under the
/// canonical filesystem model: `<root>/<source_id>/<group>/<kind_lc>_<version>.json`.
/// The caller passes the already-built relative path `<group>/<file>`.
#[must_use]
pub(crate) fn crd_cache_path(root: &Path, source_id: &str, relative_path: &str) -> PathBuf {
    root.join(source_id).join(relative_path)
}

/// Compute the sidecar path that records an authoritative HTTP 404 for a
/// schema file.
///
/// Positive cache entries remain the schema file itself. The sidecar only
/// records that the upstream source confirmed this exact file absent, so
/// callers can avoid repeating slow 404 fetches across CLI invocations.
#[must_use]
pub(crate) fn not_found_marker_path(schema_path: &Path) -> PathBuf {
    let mut file_name = schema_path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("schema"));
    file_name.push(".not-found");
    schema_path.with_file_name(file_name)
}

/// True when a prior lookup recorded an authoritative upstream 404 for this
/// exact schema file.
#[must_use]
pub fn not_found_marker_exists(schema_path: &Path) -> bool {
    not_found_marker_path(schema_path).exists()
}

/// Persist an authoritative upstream 404 for this exact schema file.
///
/// The marker is best-effort cache state: callers should still keep their
/// in-process negative cache coherent even if this write fails.
pub(crate) fn write_not_found_marker(schema_path: &Path) -> io::Result<()> {
    let marker = not_found_marker_path(schema_path);
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(marker, b"not-found\n")
}

/// Path to the per-root layout marker file.
#[must_use]
pub(crate) fn layout_marker_path(root: &Path) -> PathBuf {
    root.join(LAYOUT_MARKER_FILENAME)
}

pub(crate) fn default_cache_dir(env_var: &str, leaf: &str) -> PathBuf {
    if let Ok(path) = std::env::var(env_var) {
        return PathBuf::from(path);
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("helm-schema").join(leaf);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("helm-schema")
            .join(leaf);
    }
    PathBuf::from(".cache").join("helm-schema").join(leaf)
}

pub(crate) fn cache_root_has_legacy_layout(
    root: &Path,
    legacy_dir_name: impl Fn(&str) -> bool,
) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == LAYOUT_MARKER_FILENAME || !entry.path().is_dir() {
            continue;
        }
        if legacy_dir_name(&name) {
            return true;
        }
    }
    false
}

/// Named subdirectories of `dir`, in raw `read_dir` order. Non-UTF-8
/// names are skipped — cache namespaces, version dirs, and CRD group
/// dirs are always ASCII. Callers that emit results sort downstream.
pub(crate) fn subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            out.push((name.to_string(), path));
        }
    }
    out
}

/// Paths of `*.json` entries directly inside `dir`, in raw `read_dir`
/// order.
pub(crate) fn json_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            out.push(path);
        }
    }
    out
}
