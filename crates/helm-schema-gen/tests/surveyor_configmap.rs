#![recursion_limit = "512"]

mod common;

use helm_schema_ir::ResourceRef;
use helm_schema_k8s::{
    KubernetesJsonSchemaProvider, kubernetes_openapi::debug_materialize_schema_for_resource,
};
use serde::Deserialize;

fn helm_template_render_configmap(chart_dir: &std::path::Path) -> Result<String, String> {
    common::helm_template_render_with_args(
        chart_dir,
        Some("templates/configmap.yaml"),
        &[
            "--set",
            "config.jetstream.enabled=true",
            "--set",
            "config.jetstream.accounts[0].name=test",
            "--set",
            "config.jetstream.accounts[0].username=username",
            "--set",
            "config.jetstream.accounts[0].password=password",
            "--set",
            "config.jetstream.accounts[0].tls.secret.name=test-user-tls",
            "--set",
            "config.jetstream.accounts[0].tls.ca=ca.crt",
            "--set",
            "config.jetstream.accounts[0].tls.cert=tls.crt",
            "--set",
            "config.jetstream.accounts[0].tls.key=tls.key",
        ],
    )
}

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
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/surveyor");
    let rendered = helm_template_render_configmap(&chart_dir);
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn rendered_configmap_validates_against_upstream_k8s_schema() {
    let chart_dir = test_util::workspace_testdata().join("charts/surveyor");
    let rendered_yaml = helm_template_render_configmap(&chart_dir).expect("helm template");
    let docs = parse_yaml_documents(&rendered_yaml);
    assert!(!docs.is_empty(), "rendered YAML contained no documents");

    let cm_doc = docs
        .into_iter()
        .find(|d| d.get("kind").and_then(|v| v.as_str()) == Some("ConfigMap"))
        .expect("rendered output did not contain a ConfigMap document");

    let api_version = cm_doc
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .expect("ConfigMap manifest missing apiVersion");
    let kind = cm_doc
        .get("kind")
        .and_then(|v| v.as_str())
        .expect("ConfigMap manifest missing kind");

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

    let errors = common::validate_json_against_schema(&cm_doc, &schema);
    assert!(
        errors.is_empty(),
        "rendered ConfigMap failed upstream K8s schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
