use helm_schema_core::{ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, ValueKind, YamlPath};
use helm_schema_k8s::{Chain, K8sSchemaProvider, KubernetesJsonSchemaProvider};
use test_util::prelude::sim_assert_eq;

fn materialize_schema_for_resource(
    provider: &impl K8sSchemaProvider,
    resource: &ResourceRef,
) -> Option<serde_json::Value> {
    provider
        .lookup(resource, &YamlPath(Vec::new()))
        .into_schema_fragment()
        .map(helm_schema_core::ProviderSchemaFragment::into_schema)
}

#[test]
fn materialize_networkpolicy_v1_35() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef::concrete(
        "networking.k8s.io/v1".to_string(),
        "NetworkPolicy".to_string(),
    );

    let schema = materialize_schema_for_resource(&provider, &r).expect("materialize schema");

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/networkpolicy_v1_35_materialized.json"
    ))
    .expect("parse fixture");

    sim_assert_eq!(have: schema, want: expected);
}

#[test]
fn networkpolicy_leaf_schema_matchlabels() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef::concrete(
        "networking.k8s.io/v1".to_string(),
        "NetworkPolicy".to_string(),
    );

    let path = YamlPath(vec![
        "spec".to_string(),
        "ingress[*]".to_string(),
        "from[*]".to_string(),
        "podSelector".to_string(),
        "matchLabels".to_string(),
    ]);

    let leaf = provider
        .lookup(&r, &path)
        .into_schema_fragment()
        .expect("leaf schema")
        .into_schema();

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/networkpolicy_v1_35_leaf_matchlabels.json"
    ))
    .expect("parse fixture");

    sim_assert_eq!(have: leaf, want: expected);
}

#[test]
fn deployment_container_security_context_leaf_is_not_pod_spec() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef::concrete("apps/v1".to_string(), "Deployment".to_string());

    let path = YamlPath(vec![
        "spec".to_string(),
        "template".to_string(),
        "spec".to_string(),
        "containers[*]".to_string(),
        "securityContext".to_string(),
    ]);

    let leaf = provider
        .lookup(&r, &path)
        .into_schema_fragment()
        .expect("container securityContext leaf schema")
        .into_schema();

    assert!(
        leaf.pointer("/properties/allowPrivilegeEscalation")
            .is_some(),
        "expected container securityContext schema, got {leaf}"
    );
    assert!(
        leaf.pointer("/required")
            .and_then(serde_json::Value::as_array)
            .is_none_or(|required| {
                !required
                    .iter()
                    .any(|value| value.as_str() == Some("containers"))
            }),
        "container securityContext must not inherit PodSpec.required.containers: {leaf}"
    );
}

#[test]
fn chain_infers_networkpolicy_matchlabels_schema_from_empty_api_version() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_allow_download(true)
        .with_api_version_guess(true);
    let chain = Chain::new(vec![Box::new(provider)]).with_inference_enabled(true);

    let use_ = ProviderSchemaUse {
        value_path: "networkPolicy.ingressNSMatchLabels".to_string(),
        path: YamlPath(vec![
            "spec".to_string(),
            "ingress[*]".to_string(),
            "from[*]".to_string(),
            "namespaceSelector".to_string(),
            "matchLabels".to_string(),
        ]),
        kind: ValueKind::Fragment,
        resource: ResourceRef::concrete(String::new(), "NetworkPolicy".to_string()),
        is_self_range_collection: false,
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: Default::default(),
        outer_guards: Vec::new(),
    };

    let schema = chain
        .schema_fragment_for_use(&use_)
        .expect("chain should resolve inferred NetworkPolicy matchLabels schema")
        .into_schema();

    sim_assert_eq!(
        have: schema
            .pointer("/additionalProperties/type")
            .and_then(serde_json::Value::as_str),
        want: Some("string"),
        "expected inferred matchLabels leaf schema, got {schema}"
    );
}

/// The legacy `load_resource_doc_by_kind_scan` path is retired. The
/// new inference path (Feature D) owns the empty-`api_version` case,
/// and only fires when invoked through a `Chain` with inference
/// enabled — a single provider lookup does not resolve an empty apiVersion.
#[test]
fn kind_scan_legacy_path_retired() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef::concrete(String::new(), "NetworkPolicy".to_string());

    assert!(
        materialize_schema_for_resource(&provider, &r).is_none(),
        "single K8s provider must not resolve schemas for an empty api_version (inference is chain-level)"
    );
}
