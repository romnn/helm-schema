use crate::DefineIndex;
use helm_schema_core::{ResourceRef, YamlPath};

use super::span_collection::{ResourceSpan, collect_resource_spans};

/// Source-position index for Kubernetes resource identity claims.
///
/// Span collection owns YAML document and transparent `kind: List` descent.
/// This type only answers "which resource contains this byte position?" and
/// rebases YAML paths from transparent list envelopes onto the inner resource.
#[derive(Default, Clone, Debug)]
pub struct ResourceIdentityIndex {
    spans: Vec<ResourceSpan>,
}

impl ResourceIdentityIndex {
    #[must_use]
    pub fn from_source(source: &str, defines: &DefineIndex) -> Self {
        Self {
            spans: collect_resource_spans(source, defines),
        }
    }

    pub fn resource_at(&self, byte: usize) -> Option<&ResourceRef> {
        self.span_index_at(byte)
            .and_then(|index| self.spans.get(index))
            .map(|span| &span.resource)
    }

    pub fn rebase_path_at(&self, byte: usize, path: YamlPath) -> YamlPath {
        let Some(span) = self
            .span_index_at(byte)
            .and_then(|index| self.spans.get(index))
        else {
            return path;
        };
        if span.path_prefix.is_empty() || !path.0.starts_with(&span.path_prefix) {
            return path;
        }
        YamlPath(path.0[span.path_prefix.len()..].to_vec())
    }

    fn span_index_at(&self, byte: usize) -> Option<usize> {
        self.spans
            .iter()
            .enumerate()
            .filter(|(_, span)| span.start <= byte && byte < span.end)
            .min_by(|(_, left), (_, right)| {
                let left_len = left.end.saturating_sub(left.start);
                let right_len = right.end.saturating_sub(right.start);
                left_len
                    .cmp(&right_len)
                    .then_with(|| right.start.cmp(&left.start))
            })
            .map(|(index, _)| index)
    }
}
