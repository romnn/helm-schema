use std::collections::HashSet;
use std::fs;
use std::path::Path;

use helm_schema_core::ResourceRef;

/// Walk every CONFIGURED `<crd_cache_root>/<source_id>/<group>/`
/// directory and collect the version suffixes of files that match
/// `(group, kind_lc)` other than `requested_version`. Used by Feature C
/// to emit the informational `CrdVersionAvailableAtOtherVersions`
/// diagnostic from local-cache evidence only.
///
/// `configured_source_ids` is the set of source-id directory names the
/// caller currently has configured. Source-id dirs on disk that are not
/// in this set are skipped (Finding 2 — stale removed-mirror caches
/// MUST NOT feed live hints).
#[must_use]
pub fn collect_other_versions(
    cache_root: &Path,
    resource: &ResourceRef,
    requested_version: &str,
    configured_source_ids: &HashSet<String>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some((group, _)) = resource.api_version.split_once('/') else {
        return out;
    };
    let kind_lc = resource.kind.to_ascii_lowercase();

    let Ok(source_entries) = fs::read_dir(cache_root) else {
        return out;
    };
    for source_entry in source_entries.flatten() {
        let Some(source_id) = source_entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !configured_source_ids.contains(&source_id) {
            continue;
        }
        let group_dir = source_entry.path().join(group);
        let Ok(files) = fs::read_dir(&group_dir) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            let Some(filename) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(stem) = filename.strip_suffix(".json") else {
                continue;
            };
            let prefix = format!("{kind_lc}_");
            let Some(version) = stem.strip_prefix(&prefix) else {
                continue;
            };
            if version == requested_version {
                continue;
            }
            out.push(version.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}
