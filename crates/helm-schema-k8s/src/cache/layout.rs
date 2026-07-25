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

/// The cache root for one managed schema source when the caller configured
/// none: the source's own environment override, else the platform per-user
/// cache directory, else the system temp directory.
///
/// The explicit override is taken verbatim — like the equivalent CLI flag, a
/// relative value is the caller's deliberate choice. The fallback chain,
/// however, always yields an ABSOLUTE path: a working-directory-relative
/// default would make the cache location depend on where the process was
/// launched, so two runs over the same chart from different directories
/// would consult two different caches — and concurrent runs sharing a
/// directory would race on the same files.
pub(crate) fn default_cache_dir(env_var: &str, leaf: &str) -> PathBuf {
    if let Some(path) = env_path(env_var) {
        return path;
    }
    cache_home_for(cfg!(windows), env_path)
        .unwrap_or_else(std::env::temp_dir)
        .join("helm-schema")
        .join(leaf)
}

/// The per-user cache directory, or `None` when the platform's variables are
/// unset (a bare container, a service account with no profile).
///
/// The platform flag and environment reader are parameters so both branches
/// get unit tests on every host — the Windows branch exists precisely
/// because untested Windows-only resolution once fell through to a
/// working-directory-relative path.
fn cache_home_for(windows: bool, get: impl Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    // Relative values are ignored, mirroring the XDG rule that relative
    // basedir paths are invalid: honoring one would silently anchor the
    // "per-user" cache to the current working directory.
    let absolute = |var: &str| get(var).filter(|path| path.is_absolute());
    if windows {
        // Windows has no XDG convention: per-user, non-roaming application
        // data lives under LOCALAPPDATA, with USERPROFILE as its fallback.
        // HOME is deliberately not consulted — cmd and PowerShell leave it
        // unset while MSYS shells set it elsewhere, so honoring it would
        // move the cache depending on which shell launched the tool.
        return absolute("LOCALAPPDATA")
            .or_else(|| absolute("USERPROFILE").map(|home| home.join("AppData").join("Local")));
    }
    absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|home| home.join(".cache")))
}

/// A path from the environment, treating unset and empty alike: an exported
/// but empty variable would otherwise resolve to the working directory.
fn env_path(var: &str) -> Option<PathBuf> {
    let value = std::env::var_os(var)?;
    (!value.is_empty()).then(|| PathBuf::from(value))
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

#[cfg(test)]
#[path = "tests/layout.rs"]
mod tests;
