use helm_schema_core::ResourceRef;

use crate::builtin_groups::is_k8s_builtin_group;

/// Compute the catalog-relative path `<group>/<kind_lc>_<version>.json`.
/// Returns `None` for resources that don't belong in the CRD catalog —
/// built-in K8s API groups (see [`is_k8s_builtin_group`]) plus anything
/// without a group/version.
#[must_use]
pub fn relative_path_for_resource(resource: &ResourceRef) -> Option<String> {
    let relative_path = crate::filename::group_relative_path_for_resource(resource)?;
    let (group, _rest) = relative_path.split_once('/')?;
    if is_k8s_builtin_group(group) {
        return None;
    }
    Some(relative_path)
}

#[cfg(test)]
#[path = "tests/relative_path.rs"]
mod tests;
