use std::collections::BTreeSet;

use crate::YamlPath;

pub fn values_path_has_descendant(path: &str, sources: &BTreeSet<String>) -> bool {
    sources
        .iter()
        .any(|source| values_path_is_descendant(source, path))
}

pub fn values_path_is_descendant(path: &str, ancestor: &str) -> bool {
    let path = crate::split_value_path(path);
    let ancestor = crate::split_value_path(ancestor);
    path.len() > ancestor.len() && path.starts_with(&ancestor)
}

pub fn append_relative_path(base: &YamlPath, relative: &YamlPath) -> YamlPath {
    let mut out = base.clone();
    out.0.extend(relative.0.iter().cloned());
    out
}

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
