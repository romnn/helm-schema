use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::lookup::ProviderOrigin;

use super::candidate::{ApiVersionCandidate, InferenceSource};

/// Scan one K8s cache root across every CONFIGURED `<source_id>` and
/// every CONFIGURED `<version_dir>` for files whose
/// `x-kubernetes-group-version-kind` payload matches `kind`.
/// Returns candidates with `source: LocalCacheScan` and
/// `origin: KubernetesOpenApi`.
///
/// `configured_source_ids` is the set of source-id directory names the
/// caller currently has configured (e.g. `default` + any mirror id from
/// `--k8s-schema-mirror`). Stale source dirs left behind by a
/// previously configured mirror MUST NOT influence live inference
/// (Finding 2, round 1).
///
/// `inference_versions` is the set of `<version_dir>` names eligible
/// for inference — typically the user-EXPLICIT versions only, NOT the
/// auto-fallback escape valves. Including auto-fallback dirs would
/// surface historical apiVersions (`policy/v1beta1`, etc.) for kinds
/// whose modern version lives at the primary, producing spurious
/// `AmbiguousApiVersion` diagnostics (Finding 4, round 2).
#[must_use]
pub fn scan_k8s_cache(
    root: &Path,
    kind: &str,
    configured_source_ids: &HashSet<String>,
    inference_versions: &HashSet<String>,
) -> Vec<ApiVersionCandidate> {
    let mut out = Vec::new();
    let Ok(source_entries) = fs::read_dir(root) else {
        return out;
    };
    for source_entry in source_entries.flatten() {
        let source_path = source_entry.path();
        if !source_path.is_dir() {
            continue;
        }
        let Some(source_id) = source_entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !configured_source_ids.contains(&source_id) {
            continue;
        }
        let Ok(version_entries) = fs::read_dir(&source_path) else {
            continue;
        };
        for version_entry in version_entries.flatten() {
            let version_path = version_entry.path();
            if !version_path.is_dir() {
                continue;
            }
            let Some(version_name) = version_entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if !inference_versions.contains(&version_name) {
                continue;
            }
            let Ok(files) = fs::read_dir(&version_path) else {
                continue;
            };
            for file in files.flatten() {
                let p = file.path();
                if p.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                if let Some(api_version) = read_k8s_api_version(&p, kind) {
                    out.push(ApiVersionCandidate {
                        api_version,
                        source: InferenceSource::LocalCacheScan,
                        origin: ProviderOrigin::KubernetesOpenApi,
                    });
                }
            }
        }
    }
    out
}

/// Scan a CRD cache root across every CONFIGURED `<source_id>` for
/// files whose `(group, kind)` filename pattern matches `kind`.
/// Returns candidates with `source: LocalCacheScan` and the supplied
/// `origin`.
///
/// See [`scan_k8s_cache`] for the configured-source contract — stale
/// mirror namespaces left behind on disk do NOT contribute candidates.
#[must_use]
pub fn scan_crd_cache(
    root: &Path,
    kind: &str,
    origin: ProviderOrigin,
    configured_source_ids: &HashSet<String>,
) -> Vec<ApiVersionCandidate> {
    let mut out = Vec::new();
    let Ok(source_entries) = fs::read_dir(root) else {
        return out;
    };
    let kind_lc = kind.to_ascii_lowercase();
    for source_entry in source_entries.flatten() {
        let source_path = source_entry.path();
        if !source_path.is_dir() {
            continue;
        }
        let Some(source_id) = source_entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !configured_source_ids.contains(&source_id) {
            continue;
        }
        out.extend(scan_crd_source_dir(&source_path, &kind_lc, origin));
    }
    out
}

/// Scan a single CRD source-namespace directory (or an override root).
#[must_use]
pub fn scan_crd_source_dir(
    source_root: &Path,
    kind_lc: &str,
    origin: ProviderOrigin,
) -> Vec<ApiVersionCandidate> {
    let mut out = Vec::new();
    let Ok(group_entries) = fs::read_dir(source_root) else {
        return out;
    };
    for group_entry in group_entries.flatten() {
        let group_path = group_entry.path();
        if !group_path.is_dir() {
            continue;
        }
        let Some(group_name) = group_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let Ok(files) = fs::read_dir(&group_path) else {
            continue;
        };
        for file in files.flatten() {
            let p = file.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Some(filename) = p.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if let Some(version) = match_crd_filename(filename, kind_lc) {
                out.push(ApiVersionCandidate {
                    api_version: format!("{group_name}/{version}"),
                    source: InferenceSource::LocalCacheScan,
                    origin,
                });
            }
        }
    }
    out
}

fn read_k8s_api_version(path: &Path, kind: &str) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let doc: Value = serde_json::from_slice(&bytes).ok()?;
    let entries = doc.get("x-kubernetes-group-version-kind")?.as_array()?;
    for entry in entries {
        let entry_kind = entry.get("kind").and_then(|v| v.as_str())?;
        if entry_kind != kind {
            continue;
        }
        let group = entry.get("group").and_then(|v| v.as_str()).unwrap_or("");
        let version = entry.get("version").and_then(|v| v.as_str()).unwrap_or("");
        if version.is_empty() {
            continue;
        }
        return Some(if group.is_empty() {
            version.to_string()
        } else {
            format!("{group}/{version}")
        });
    }
    None
}

fn match_crd_filename(filename: &str, kind_lc: &str) -> Option<String> {
    let prefix = format!("{kind_lc}_");
    let stem = filename.strip_suffix(".json")?;
    let version = stem.strip_prefix(&prefix)?;
    if version.is_empty() {
        return None;
    }
    Some(version.to_string())
}
