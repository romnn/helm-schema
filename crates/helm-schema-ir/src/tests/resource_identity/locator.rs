use helm_schema_ast::{AttributionIndex, DefineIndex, build_attribution_index_with_resources};
use indoc::indoc;

use test_util::prelude::sim_assert_eq;

fn attribution_index(source: &str) -> AttributionIndex {
    let tree = helm_schema_ast::parse_go_template(source).expect("parse template");
    build_attribution_index_with_resources(source, tree.root_node(), &DefineIndex::new())
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
