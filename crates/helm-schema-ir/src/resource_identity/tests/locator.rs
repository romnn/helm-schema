use helm_schema_ast::DefineIndex;
use indoc::indoc;

use super::super::ResourceIdentityIndex;
use test_util::prelude::sim_assert_eq;

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
    let mut locator = ResourceIdentityIndex::from_source(source, &DefineIndex::new());

    locator.advance_to(source.find("first").expect("first marker"));
    let first = locator.current_resource().expect("first resource");
    sim_assert_eq!(have: first.kind, want: "ConfigMap");
    sim_assert_eq!(have: first.api_version, want: "v1");
    assert!(first.api_version_candidates.is_empty());

    locator.advance_to(source.find("replicas").expect("replicas marker"));
    let second = locator.current_resource().expect("second resource");
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
    let mut locator = ResourceIdentityIndex::from_source(source, &DefineIndex::new());
    sim_assert_eq!(
        have: locator.span_count(),
        want: 2,
        "list envelope should produce one span per inner resource"
    );

    locator.advance_to(source.find("host").expect("host marker"));
    let ingress = locator.current_resource().expect("ingress resource");
    sim_assert_eq!(have: ingress.kind, want: "Ingress");
    sim_assert_eq!(have: ingress.api_version, want: "networking.k8s.io/v1");
    sim_assert_eq!(
        have: locator.rebase_path(crate::YamlPath(vec![
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

    locator.advance_to(source.find("port").expect("port marker"));
    let service = locator.current_resource().expect("service resource");
    sim_assert_eq!(have: service.kind, want: "Service");
    sim_assert_eq!(have: service.api_version, want: "v1");
    sim_assert_eq!(
        have: locator.rebase_path(crate::YamlPath(vec![
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

    locator.advance_to(source.find("kind: List").expect("list marker"));
    assert!(
        locator.current_resource().is_none(),
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
    let mut locator = ResourceIdentityIndex::from_source(source, &DefineIndex::new());
    sim_assert_eq!(
        have: locator.span_count(),
        want: 1,
        "ranged list envelope should produce the inner resource span"
    );

    locator.advance_to(source.find("host").expect("host marker"));
    let ingress = locator.current_resource().expect("ingress resource");
    sim_assert_eq!(have: ingress.kind, want: "Ingress");
    sim_assert_eq!(have: ingress.api_version, want: "networking.k8s.io/v1");
    sim_assert_eq!(
        have: locator.rebase_path(crate::YamlPath(vec![
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
    let mut locator = ResourceIdentityIndex::from_source(source, &DefineIndex::new());
    sim_assert_eq!(
        have: locator.span_count(),
        want: 1,
        "only the exact kubernetes v1/list envelope should be transparent"
    );

    locator.advance_to(source.find("port").expect("port marker"));
    let resource = locator.current_resource().expect("outer resource");
    sim_assert_eq!(have: resource.kind, want: "List");
    sim_assert_eq!(have: resource.api_version, want: "example.com/v1");
    sim_assert_eq!(
        have: locator.rebase_path(crate::YamlPath(vec![
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
