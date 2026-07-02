use std::collections::HashSet;
use std::path::Path;

use helm_schema_core::ResourceRef;

use crate::cache::{json_files, subdirs};
use crate::inference::cache_scan::match_crd_filename;

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

    for (source_id, source_path) in subdirs(cache_root) {
        if !configured_source_ids.contains(&source_id) {
            continue;
        }
        for path in json_files(&source_path.join(group)) {
            let Some(filename) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(version) = match_crd_filename(filename, &kind_lc) else {
                continue;
            };
            if version != requested_version {
                out.push(version);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}
