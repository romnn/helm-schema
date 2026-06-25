use crate::DefineIndex;
use helm_schema_core::ResourceRef;

use super::list_envelope::list_item_sources;
use super::manifest_resource::{detect_manifest_resource, is_kubernetes_list_envelope};
use super::source_documents::document_spans;

#[derive(Clone, Debug)]
pub(super) struct ResourceSpan {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) resource: ResourceRef,
    pub(super) path_prefix: Vec<String>,
}

pub(super) fn collect_resource_spans(source: &str, defines: &DefineIndex) -> Vec<ResourceSpan> {
    let mut spans = Vec::new();
    for (start, end) in document_spans(source) {
        let Some(document_source) = source.get(start..end) else {
            continue;
        };
        spans.extend(resource_spans_for_manifest_source(
            document_source,
            start,
            start,
            end,
            Vec::new(),
            defines,
        ));
    }
    spans.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.end.cmp(&right.end))
    });
    spans
}

fn resource_spans_for_manifest_source(
    source: &str,
    base_offset: usize,
    span_start: usize,
    span_end: usize,
    path_prefix: Vec<String>,
    defines: &DefineIndex,
) -> Vec<ResourceSpan> {
    let Some(resource) = detect_manifest_resource(source, defines) else {
        return Vec::new();
    };

    if is_kubernetes_list_envelope(&resource) {
        return list_item_sources(source, base_offset, path_prefix)
            .into_iter()
            .flat_map(|item| {
                resource_spans_for_manifest_source(
                    item.source,
                    item.start,
                    item.start,
                    item.end,
                    item.path_prefix,
                    defines,
                )
            })
            .collect();
    }

    vec![ResourceSpan {
        start: span_start,
        end: span_end,
        resource,
        path_prefix,
    }]
}
