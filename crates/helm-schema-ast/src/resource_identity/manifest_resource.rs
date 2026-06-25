use crate::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_core::ResourceRef;

use super::ResourceIdentityDetector;

pub(super) fn detect_manifest_resource(source: &str, defines: &DefineIndex) -> Option<ResourceRef> {
    if let Some(resource) = TreeSitterParser
        .parse(source)
        .ok()
        .and_then(|ast| ResourceIdentityDetector::new(defines).detect(&ast))
    {
        return Some(resource);
    }

    let normalized = normalize_sequence_item_source(source);
    if normalized == source {
        return None;
    }
    TreeSitterParser
        .parse(&normalized)
        .ok()
        .and_then(|ast| ResourceIdentityDetector::new(defines).detect(&ast))
}

pub(super) fn is_kubernetes_list_envelope(resource: &ResourceRef) -> bool {
    resource.kind == "List"
        && resource.api_version == "v1"
        && resource.api_version_candidates.is_empty()
        && resource.api_version_branches.is_empty()
}

fn normalize_sequence_item_source(source: &str) -> String {
    let mut lines = source.lines();
    let Some(first) = lines.next() else {
        return source.to_string();
    };
    let rest = lines.collect::<Vec<_>>();
    // Tree-sitter gives us the exact mapping node inside a YAML sequence item.
    // The first key starts immediately after `- `, while continuation lines
    // still carry their original document indentation. Dedent only that shared
    // continuation indentation so the item parses as a standalone resource.
    let Some(indent) = rest
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .filter(|indent| *indent > 0)
        .min()
    else {
        return source.to_string();
    };

    let mut normalized = String::with_capacity(source.len());
    normalized.push_str(first);
    for line in rest {
        normalized.push('\n');
        let line_indent = line.len() - line.trim_start_matches(' ').len();
        if line_indent >= indent {
            normalized.push_str(&line[indent..]);
        } else {
            normalized.push_str(line);
        }
    }
    if source.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}
