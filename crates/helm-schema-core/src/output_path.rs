use std::collections::BTreeSet;

use crate::YamlPath;

/// Structural path segment for the value of a rendered mapping entry whose
/// key is supplied by a template expression.
pub const DYNAMIC_MAPPING_VALUE_SEGMENT: &str = "{*}";

/// Reports whether `sources` contains a strict descendant of `path`.
#[must_use]
pub fn values_path_has_descendant(path: &str, sources: &BTreeSet<String>) -> bool {
    sources
        .iter()
        .any(|source| values_path_is_descendant(source, path))
}

/// Reports whether `path` is a strict segmented descendant of `ancestor`.
#[must_use]
pub fn values_path_is_descendant(path: &str, ancestor: &str) -> bool {
    let path = crate::split_value_path(path);
    let ancestor = crate::split_value_path(ancestor);
    path.len() > ancestor.len() && path.starts_with(&ancestor)
}

/// Appends a rendered YAML path relative to a base path.
#[must_use]
pub fn append_relative_path(base: &YamlPath, relative: &YamlPath) -> YamlPath {
    let mut out = base.clone();
    out.0.extend(relative.0.iter().cloned());
    out
}

/// Marks the final path segment as a sequence-item collection slot.
#[must_use]
pub fn sequence_item_path(relative_path: &YamlPath) -> YamlPath {
    let mut path = relative_path.clone();
    if let Some(last) = path.0.last_mut() {
        if !last.ends_with("[*]") {
            last.push_str("[*]");
        }
    } else {
        path.0.push("[*]".to_string());
    }
    path
}

/// Appends the structural dynamic-key value segment to a rendered YAML path.
#[must_use]
pub fn dynamic_mapping_value_path(relative_path: &YamlPath) -> YamlPath {
    let mut path = relative_path.clone();
    path.0.push(DYNAMIC_MAPPING_VALUE_SEGMENT.to_string());
    path
}
