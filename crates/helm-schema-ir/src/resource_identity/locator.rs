use helm_schema_ast::DefineIndex;

use super::span_collection::{ResourceSpan, collect_resource_spans};
use crate::{ResourceRef, YamlPath};

/// Source-position index for Kubernetes resource identity claims.
///
/// Span collection owns YAML document and transparent `kind: List` descent.
/// This type only answers "which resource contains this byte position?" and
/// rebases YAML paths from transparent list envelopes onto the inner resource.
#[derive(Default, Clone, Debug)]
pub(crate) struct ResourceIdentityIndex {
    spans: Vec<ResourceSpan>,
    current_span: Option<usize>,
}

impl ResourceIdentityIndex {
    #[must_use]
    pub(crate) fn from_source(source: &str, defines: &DefineIndex) -> Self {
        Self {
            spans: collect_resource_spans(source, defines),
            current_span: None,
        }
    }

    pub(crate) fn advance_to(&mut self, byte: usize) {
        self.current_span = self
            .spans
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
            .map(|(index, _)| index);
    }

    pub(crate) fn current_resource(&self) -> Option<&ResourceRef> {
        self.current_span
            .and_then(|index| self.spans.get(index))
            .map(|span| &span.resource)
    }

    pub(crate) fn rebase_path(&self, path: YamlPath) -> YamlPath {
        let Some(span) = self.current_span.and_then(|index| self.spans.get(index)) else {
            return path;
        };
        if span.path_prefix.is_empty() || !path.0.starts_with(&span.path_prefix) {
            return path;
        }
        YamlPath(path.0[span.path_prefix.len()..].to_vec())
    }

    #[cfg(test)]
    pub(super) fn span_count(&self) -> usize {
        self.spans.len()
    }
}
