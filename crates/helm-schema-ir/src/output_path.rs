use std::collections::BTreeSet;

use crate::YamlPath;

pub(crate) fn values_path_has_descendant(path: &str, sources: &BTreeSet<String>) -> bool {
    sources
        .iter()
        .any(|source| values_path_is_strict_descendant(source, path))
}

fn values_path_is_strict_descendant(path: &str, ancestor: &str) -> bool {
    if ancestor.trim().is_empty() {
        return !path.trim().is_empty();
    }

    path.strip_prefix(ancestor)
        .is_some_and(|rest| rest.starts_with('.'))
}

pub(crate) fn append_relative_path(base: &YamlPath, relative: &YamlPath) -> YamlPath {
    let mut out = base.clone();
    out.0.extend(relative.0.iter().cloned());
    out
}

pub(crate) fn sequence_item_path(relative_path: &YamlPath) -> YamlPath {
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
