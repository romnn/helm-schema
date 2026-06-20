#![recursion_limit = "512"]

mod common;

use helm_schema_ir::ResourceRef;
use helm_schema_k8s::{
    KubernetesJsonSchemaProvider, kubernetes_openapi::debug_materialize_schema_for_resource,
};
use serde::Deserialize;

fn parse_yaml_documents(yaml: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(yaml) {
        let v = serde_json::Value::deserialize(doc).expect("parse YAML document as JSON");
        if v.is_null() {
            continue;
        }
        out.push(v);
    }
    out
}

#[test]
fn rendered_ingress_validates_against_upstream_k8s_schema() {
    let chart_dir = test_util::workspace_testdata().join("charts/zalando-postgres-operator-ui");
    let rendered_yaml = common::helm_template_render_with_args(
        &chart_dir,
        Some("templates/ingress.yaml"),
        &["--set", "ingress.enabled=true", "--kube-version", "1.29.0"],
    )
    .expect("helm template");
    let docs = parse_yaml_documents(&rendered_yaml);
    assert!(!docs.is_empty(), "rendered YAML contained no documents");

    let ingress_doc = docs
        .into_iter()
        .find(|d| d.get("kind").and_then(|v| v.as_str()) == Some("Ingress"))
        .expect("rendered output did not contain an Ingress document");

    let api_version = ingress_doc
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .expect("Ingress manifest missing apiVersion");
    let kind = ingress_doc
        .get("kind")
        .and_then(|v| v.as_str())
        .expect("Ingress manifest missing kind");

    let resource = ResourceRef {
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = debug_materialize_schema_for_resource(&provider, &resource)
        .expect("load upstream k8s schema for rendered resource");

    let schema = match schema {
        serde_json::Value::Object(mut obj) => {
            let _ = obj.remove("$schema");
            serde_json::Value::Object(obj)
        }
        other => other,
    };

    let errors = common::validate_json_against_schema(&ingress_doc, &schema);
    assert!(
        errors.is_empty(),
        "rendered Ingress failed upstream K8s schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
