use helm_schema_core::ResourceRef;

use crate::builtin_groups::is_k8s_builtin_group;

/// Compute the catalog-relative path `<group>/<kind_lc>_<version>.json`.
/// Returns `None` for resources that don't belong in the CRD catalog —
/// built-in K8s API groups (see [`is_k8s_builtin_group`]) plus anything
/// without a group/version.
#[must_use]
pub fn relative_path_for_resource(resource: &ResourceRef) -> Option<String> {
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

    if is_k8s_builtin_group(group) {
        return None;
    }

    let kind = kind.to_ascii_lowercase();
    Some(format!("{group}/{kind}_{version}.json"))
}

#[cfg(test)]
#[path = "tests/relative_path.rs"]
mod tests;
