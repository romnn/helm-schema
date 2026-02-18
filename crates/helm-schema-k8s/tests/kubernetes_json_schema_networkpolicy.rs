use helm_schema_ir::{ResourceRef, YamlPath};
use helm_schema_k8s::{K8sSchemaProvider, KubernetesJsonSchemaProvider};

#[test]
fn materialize_networkpolicy_v1_35() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef {
        api_version: "networking.k8s.io/v1".to_string(),
        kind: "NetworkPolicy".to_string(),
        api_version_candidates: Vec::new(),
    };

    let schema = provider
        .materialize_schema_for_resource(&r)
        .expect("materialize schema");

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/networkpolicy_v1_35_materialized.json"
    ))
    .expect("parse fixture");

    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn networkpolicy_leaf_schema_matchlabels() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef {
        api_version: "networking.k8s.io/v1".to_string(),
        kind: "NetworkPolicy".to_string(),
        api_version_candidates: Vec::new(),
    };

    let path = YamlPath(vec![
        "spec".to_string(),
        "ingress[*]".to_string(),
        "from[*]".to_string(),
        "podSelector".to_string(),
        "matchLabels".to_string(),
    ]);

    let leaf = provider
        .schema_for_resource_path(&r, &path)
        .expect("leaf schema");

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/networkpolicy_v1_35_leaf_matchlabels.json"
    ))
    .expect("parse fixture");

    similar_asserts::assert_eq!(leaf, expected);
}

#[test]
fn networkpolicy_by_kind_scan_when_api_version_missing() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let r = ResourceRef {
        api_version: String::new(),
        kind: "NetworkPolicy".to_string(),
        api_version_candidates: Vec::new(),
    };

    let schema = provider
        .materialize_schema_for_resource(&r)
        .expect("materialize schema by kind");

    assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
    assert!(
        schema
            .get("properties")
            .and_then(|v| v.as_object())
            .is_some()
    );
}
