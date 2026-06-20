use helm_schema_core::{ProviderSchemaUse, ResourceRef, ValueKind, YamlPath};
use helm_schema_k8s::{
    Chain, K8sSchemaProvider, KubernetesJsonSchemaProvider,
    kubernetes_openapi::debug_materialize_schema_for_resource,
};
use test_util::prelude::sim_assert_eq;

#[test]
fn materialize_networkpolicy_v1_35() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef {
        api_version: "networking.k8s.io/v1".to_string(),
        kind: "NetworkPolicy".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    let schema = debug_materialize_schema_for_resource(&provider, &r).expect("materialize schema");

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/networkpolicy_v1_35_materialized.json"
    ))
    .expect("parse fixture");

    sim_assert_eq!(have: schema, want: expected);
}

#[test]
fn networkpolicy_leaf_schema_matchlabels() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef {
        api_version: "networking.k8s.io/v1".to_string(),
        kind: "NetworkPolicy".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    let path = YamlPath(vec![
        "spec".to_string(),
        "ingress[*]".to_string(),
        "from[*]".to_string(),
        "podSelector".to_string(),
        "matchLabels".to_string(),
    ]);

    let leaf = provider
        .schema_fragment_for_resource_path(&r, &path)
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

    let r = ResourceRef {
        api_version: "apps/v1".to_string(),
        kind: "Deployment".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    let path = YamlPath(vec![
        "spec".to_string(),
        "template".to_string(),
        "spec".to_string(),
        "containers[*]".to_string(),
        "securityContext".to_string(),
    ]);

    let leaf = provider
        .schema_fragment_for_resource_path(&r, &path)
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
        resource: ResourceRef {
            api_version: String::new(),
            kind: "NetworkPolicy".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        },
        is_self_range_collection: false,
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
/// enabled — a single provider's `schema_fragment_for_resource_path` returns
/// `None` for an empty api_version.
#[test]
fn kind_scan_legacy_path_retired() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef {
        api_version: String::new(),
        kind: "NetworkPolicy".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    assert!(
        debug_materialize_schema_for_resource(&provider, &r).is_none(),
        "single K8s provider must not resolve schemas for an empty api_version (inference is chain-level)"
    );
}
