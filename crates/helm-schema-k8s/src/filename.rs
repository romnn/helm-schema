//! Filename / api-version ordering helpers shared across providers.
//!
//! Lives in the crate root (not under `kubernetes_openapi/`) because
//! the CRD provider's diagnostics + the chain's MissingSchema payload
//! also derive candidate filenames from a `ResourceRef`.

use helm_schema_core::ResourceRef;

/// Lower is better — drives `ordered_api_versions_for_resource`.
///
///   - 0 = stable, 1 = beta, 2 = alpha, 3 = unknown.
///   - Prefer non-`extensions` API groups when all else is equal.
///   - Prefer higher major versions and higher beta/alpha iterations.
fn api_version_rank(api_version: &str) -> (u8, u8, i32, i32) {
    let (group, ver) = api_version.split_once('/').unwrap_or(("", api_version));

    let is_extensions = u8::from(group == "extensions");

    let (stability, pre_iter) = if ver.contains("alpha") {
        let it = ver
            .split("alpha")
            .nth(1)
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        (2u8, it)
    } else if ver.contains("beta") {
        let it = ver
            .split("beta")
            .nth(1)
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        (1u8, it)
    } else if ver.starts_with('v')
        && ver
            .get(1..)
            .and_then(|s| s.chars().next())
            .is_some_and(|c| c.is_ascii_digit())
        && ver
            .get(1..)
            .is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
    {
        (0u8, 0)
    } else {
        (3u8, 0)
    };

    let major = ver
        .strip_prefix('v')
        .and_then(|s| {
            s.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<i32>()
                .ok()
        })
        .unwrap_or(0);

    (stability, is_extensions, -major, -pre_iter)
}

/// Order resource's known apiVersions by `api_version_rank`. The
/// primary `api_version` is preferred when present; otherwise the
/// `api_version_candidates` are ranked among themselves.
#[must_use]
pub fn ordered_api_versions_for_resource(r: &ResourceRef) -> Vec<&str> {
    let mut versions: Vec<&str> = Vec::new();
    if !r.api_version.trim().is_empty() {
        versions.push(r.api_version.as_str());
    }
    for v in &r.api_version_candidates {
        if !v.trim().is_empty() {
            versions.push(v.as_str());
        }
    }
    if versions.is_empty() {
        versions.push(r.api_version.as_str());
    }

    versions.sort_by_key(|v| api_version_rank(v));
    versions.dedup();
    versions
}

#[must_use]
pub fn candidate_filenames_for_resource(resource: &ResourceRef) -> Vec<String> {
    let kind = resource.kind.to_ascii_lowercase();
    let (group, version) = match resource.api_version.split_once('/') {
        Some((g, v)) => (g.to_ascii_lowercase(), v.to_ascii_lowercase()),
        None => (String::new(), resource.api_version.to_ascii_lowercase()),
    };

    let mut out = Vec::new();
    if group.is_empty() {
        out.push(format!("{kind}-{version}.json"));
        return out;
    }

    let dashed_full_group = group.replace('.', "-");
    let group_prefix = group.split('.').next().unwrap_or(&group).to_string();

    if group.ends_with(".k8s.io") {
        out.push(format!("{kind}-{group_prefix}-{version}.json"));
    }

    out.push(format!("{kind}-{dashed_full_group}-{version}.json"));

    if !group.ends_with(".k8s.io") {
        out.push(format!("{kind}-{group_prefix}-{version}.json"));
    }

    out.dedup();
    out
}

#[must_use]
pub fn filename_for_resource(resource: &ResourceRef) -> String {
    candidate_filenames_for_resource(resource)
        .into_iter()
        .next()
        .unwrap_or_else(|| "unknown.json".to_string())
}

/// Compute the catalog-relative path `<group>/<kind_lc>_<version>.json` for a
/// grouped resource. Returns `None` for resources without a `group/version`
/// apiVersion. Callers that only serve custom resources filter built-in K8s
/// groups on top of this.
pub(crate) fn group_relative_path_for_resource(resource: &ResourceRef) -> Option<String> {
    let api_version = resource.api_version.trim();
    let kind = resource.kind.trim();
    if api_version.is_empty() || kind.is_empty() {
        return None;
    }
    let (group, version) = api_version.split_once('/')?;
    let group = group.trim();
    let version = version.trim();
    if group.is_empty() || version.is_empty() {
        return None;
    }
    let kind = kind.to_ascii_lowercase();
    Some(format!("{group}/{kind}_{version}.json"))
}
