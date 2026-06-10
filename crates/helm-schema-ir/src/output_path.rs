use std::collections::BTreeSet;

use helm_schema_ast::HelmAst;

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

pub(crate) fn pending_mapping_key_path(
    node: &HelmAst,
    relative_path: &YamlPath,
) -> Option<YamlPath> {
    let segment = match node {
        HelmAst::Mapping { items } => {
            let [HelmAst::Pair { key, value: None }] = items.as_slice() else {
                return None;
            };
            key_segment(key)?
        }
        HelmAst::Pair { key, value: None } => key_segment(key)?,
        _ => return None,
    };
    let mut path = relative_path.clone();
    path.0.push(segment);
    Some(path)
}

pub(crate) fn trailing_pending_mapping_key_path(
    node: &HelmAst,
    relative_path: &YamlPath,
) -> Option<YamlPath> {
    match node {
        HelmAst::Document { items }
        | HelmAst::Mapping { items }
        | HelmAst::Define { body: items, .. }
        | HelmAst::Block { body: items, .. } => items
            .last()
            .and_then(|item| trailing_pending_mapping_key_path(item, relative_path)),
        HelmAst::Pair { key, value } => {
            let segment = key_segment(key)?;
            let mut value_path = relative_path.clone();
            value_path.0.push(segment);
            match value {
                Some(value) => trailing_pending_mapping_key_path(value, &value_path),
                None => Some(value_path),
            }
        }
        HelmAst::Sequence { items } => {
            let item_path = sequence_item_path(relative_path);
            items
                .last()
                .and_then(|item| trailing_pending_mapping_key_path(item, &item_path))
        }
        HelmAst::If {
            then_branch,
            else_branch,
            ..
        } => then_branch
            .last()
            .and_then(|item| trailing_pending_mapping_key_path(item, relative_path))
            .or_else(|| {
                else_branch
                    .last()
                    .and_then(|item| trailing_pending_mapping_key_path(item, relative_path))
            }),
        HelmAst::With {
            body, else_branch, ..
        }
        | HelmAst::Range {
            body, else_branch, ..
        } => body
            .last()
            .and_then(|item| trailing_pending_mapping_key_path(item, relative_path))
            .or_else(|| {
                else_branch
                    .last()
                    .and_then(|item| trailing_pending_mapping_key_path(item, relative_path))
            }),
        HelmAst::Scalar { .. } | HelmAst::HelmExpr { .. } | HelmAst::HelmComment { .. } => None,
    }
}

pub(crate) fn key_segment(node: &HelmAst) -> Option<String> {
    match node {
        HelmAst::Scalar { text } => Some(text.clone()),
        _ => None,
    }
}
