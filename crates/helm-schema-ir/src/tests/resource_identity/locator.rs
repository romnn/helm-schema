use helm_schema_ast::{DefineIndex, ResourceSpan};
use helm_schema_core::ResourceRef;
use indoc::indoc;

use test_util::prelude::sim_assert_eq;

/// Test-side span queries with the interpreter's site rules: the smallest
/// resource span containing a byte wins, a window spanning several distinct
/// resources claims none, and List-item spans rebase emitted paths below
/// their `items[*]` prefix.
struct SpanLocator {
    spans: Vec<ResourceSpan>,
}

impl SpanLocator {
    fn span_at(&self, byte: usize) -> Option<&ResourceSpan> {
        self.spans
            .iter()
            .filter(|span| span.start <= byte && byte < span.end)
            .min_by(|left, right| {
                let left_len = left.end.saturating_sub(left.start);
                let right_len = right.end.saturating_sub(right.start);
                left_len
                    .cmp(&right_len)
                    .then_with(|| right.start.cmp(&left.start))
            })
    }

    fn resource_at(&self, byte: usize) -> Option<&ResourceRef> {
        self.span_at(byte).map(|span| &span.resource)
    }

    fn rebase_path_at(&self, byte: usize, path: crate::YamlPath) -> crate::YamlPath {
        let Some(span) = self.span_at(byte) else {
            return path;
        };
        if span.path_prefix.is_empty() || !path.0.starts_with(&span.path_prefix) {
            return path;
        }
        crate::YamlPath(path.0[span.path_prefix.len()..].to_vec())
    }

    fn single_resource_in_span(&self, start: usize, end: usize) -> Option<&ResourceRef> {
        let mut resource = None;
        for span in &self.spans {
            if span.start >= end || start >= span.end {
                continue;
            }
            match resource {
                Some(existing) if existing != &span.resource => return None,
                Some(_) => {}
                None => resource = Some(&span.resource),
            }
        }
        resource
    }
}

fn attribution_index(source: &str) -> SpanLocator {
    let tree = helm_schema_ast::parse_go_template(source).expect("parse template");
    let defines = DefineIndex::new();
    let analysis_db = crate::analysis_db::IrAnalysisDb::new(&defines);
    let document = helm_schema_syntax::TemplatedDocument::parse_with_root(source, tree.root_node());
    SpanLocator {
        spans: crate::resource_identity::collect_resource_spans(&document, &analysis_db),
    }
}

#[test]
fn resource_locator_keeps_multi_document_resources_separate() {
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          first: "{{ .Values.first }}"
        ---
        apiVersion: apps/v1
        kind: Deployment
        spec:
          replicas: {{ .Values.replicas }}
    "#};
    let locator = attribution_index(source);

    let first_byte = source.find("first").expect("first marker");
    let first = locator.resource_at(first_byte).expect("first resource");
    sim_assert_eq!(have: first.kind, want: "ConfigMap");
    sim_assert_eq!(have: first.api_version, want: "v1");
    assert!(first.api_version_candidates.is_empty());

    let second_byte = source.find("replicas").expect("replicas marker");
    let second = locator.resource_at(second_byte).expect("second resource");
    sim_assert_eq!(have: second.kind, want: "Deployment");
    sim_assert_eq!(have: second.api_version, want: "apps/v1");
    assert!(second.api_version_candidates.is_empty());
}

#[test]
fn resource_locator_descends_into_list_items_and_rebases_paths() {
    let source = indoc! {r#"
        apiVersion: v1
        kind: List
        items:
          - apiVersion: networking.k8s.io/v1
            kind: Ingress
            spec:
              rules:
                - host: {{ .Values.host | quote }}
          - apiVersion: v1
            kind: Service
            spec:
              ports:
                - port: {{ .Values.port }}
    "#};
    let locator = attribution_index(source);

    let host_byte = source.find("host").expect("host marker");
    let ingress = locator.resource_at(host_byte).expect("ingress resource");
    sim_assert_eq!(have: ingress.kind, want: "Ingress");
    sim_assert_eq!(have: ingress.api_version, want: "networking.k8s.io/v1");
    sim_assert_eq!(
        have: locator.rebase_path_at(host_byte, crate::YamlPath(vec![
            "items[*]".to_string(),
            "spec".to_string(),
            "rules[*]".to_string(),
            "host".to_string(),
        ])),
        want: crate::YamlPath(vec![
            "spec".to_string(),
            "rules[*]".to_string(),
            "host".to_string(),
        ])
    );

    let port_byte = source.find("port").expect("port marker");
    let service = locator.resource_at(port_byte).expect("service resource");
    sim_assert_eq!(have: service.kind, want: "Service");
    sim_assert_eq!(have: service.api_version, want: "v1");
    sim_assert_eq!(
        have: locator.rebase_path_at(port_byte, crate::YamlPath(vec![
            "items[*]".to_string(),
            "spec".to_string(),
            "ports[*]".to_string(),
            "port".to_string(),
        ])),
        want: crate::YamlPath(vec![
            "spec".to_string(),
            "ports[*]".to_string(),
            "port".to_string(),
        ])
    );

    let list_byte = source.find("kind: List").expect("list marker");
    assert!(
        locator.resource_at(list_byte).is_none(),
        "the transparent list wrapper must not become the current resource"
    );
}

#[test]
fn resource_locator_reports_only_single_resource_control_spans() {
    let single = indoc! {r#"
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: ConfigMap
        data:
          name: {{ .Values.name }}
        {{- end }}
    "#};
    let locator = attribution_index(single);
    let resource = locator
        .single_resource_in_span(0, single.len())
        .expect("single resource");
    sim_assert_eq!(have: resource.kind.as_str(), want: "ConfigMap");
    sim_assert_eq!(have: resource.api_version.as_str(), want: "v1");

    let multiple = indoc! {r#"
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: ConfigMap
        ---
        apiVersion: v1
        kind: Secret
        {{- end }}
    "#};
    let locator = attribution_index(multiple);
    assert!(locator.single_resource_in_span(0, multiple.len()).is_none());
}

#[test]
fn resource_locator_descends_into_ranged_list_items() {
    let source = indoc! {r#"
        apiVersion: v1
        kind: List
        items:
        {{- range $index, $replica := until (.Values.replicas | int) }}
          - apiVersion: networking.k8s.io/v1
            kind: Ingress
            spec:
              rules:
                - host: {{ $.Values.host | quote }}
        {{- end }}
    "#};
    let locator = attribution_index(source);

    let host_byte = source.find("host").expect("host marker");
    let ingress = locator.resource_at(host_byte).expect("ingress resource");
    sim_assert_eq!(have: ingress.kind, want: "Ingress");
    sim_assert_eq!(have: ingress.api_version, want: "networking.k8s.io/v1");
    sim_assert_eq!(
        have: locator.rebase_path_at(host_byte, crate::YamlPath(vec![
            "items[*]".to_string(),
            "spec".to_string(),
            "rules[*]".to_string(),
            "host".to_string(),
        ])),
        want: crate::YamlPath(vec![
            "spec".to_string(),
            "rules[*]".to_string(),
            "host".to_string(),
        ])
    );
}

// Pins the Stage-A3 CST descent improvement: the old tree-sitter walker's
// ERROR-node recovery failed on the indented item fragment of a nested
// List envelope and silently dropped every resource in the document.
#[test]
fn resource_locator_descends_into_nested_list_envelopes() {
    let source = indoc! {r#"
        apiVersion: v1
        kind: List
        items:
          - apiVersion: v1
            kind: List
            items:
              - apiVersion: v1
                kind: ConfigMap
                data:
                  key: {{ .Values.key }}
    "#};
    let locator = attribution_index(source);

    let key_byte = source.find("key").expect("key marker");
    let inner = locator.resource_at(key_byte).expect("inner resource");
    sim_assert_eq!(have: inner.kind, want: "ConfigMap");
    sim_assert_eq!(have: inner.api_version, want: "v1");
    sim_assert_eq!(
        have: locator.rebase_path_at(key_byte, crate::YamlPath(vec![
            "items[*]".to_string(),
            "items[*]".to_string(),
            "data".to_string(),
            "key".to_string(),
        ])),
        want: crate::YamlPath(vec!["data".to_string(), "key".to_string()])
    );
}

#[test]
fn resource_locator_keeps_non_kubernetes_list_kind_as_resource() {
    let source = indoc! {r#"
        apiVersion: example.com/v1
        kind: List
        items:
          - apiVersion: v1
            kind: Service
            spec:
              ports:
                - port: {{ .Values.port }}
    "#};
    let locator = attribution_index(source);

    let port_byte = source.find("port").expect("port marker");
    let resource = locator.resource_at(port_byte).expect("outer resource");
    sim_assert_eq!(have: resource.kind, want: "List");
    sim_assert_eq!(have: resource.api_version, want: "example.com/v1");
    sim_assert_eq!(
        have: locator.rebase_path_at(port_byte, crate::YamlPath(vec![
            "items[*]".to_string(),
            "spec".to_string(),
            "ports[*]".to_string(),
            "port".to_string(),
        ])),
        want: crate::YamlPath(vec![
            "items[*]".to_string(),
            "spec".to_string(),
            "ports[*]".to_string(),
            "port".to_string(),
        ])
    );
}
