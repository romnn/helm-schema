#![recursion_limit = "512"]

mod common;

use helm_schema_ir::ResourceRef;
use helm_schema_k8s::{
    KubernetesJsonSchemaProvider, kubernetes_openapi::debug_materialize_schema_for_resource,
};
use serde::Deserialize;

fn helm_template_render_rbac(
    chart_dir: &std::path::Path,
    cluster_scoped: bool,
) -> Result<String, String> {
    let extra_args = if cluster_scoped {
        vec!["--set", "clusterScoped=true"]
    } else {
        Vec::new()
    };
    common::helm_template_render_with_args(&chart_dir, Some("templates/rbac.yaml"), &extra_args)
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
fn helm_template_renders_successfully_default_values() {
    let chart_dir = test_util::workspace_testdata().join("charts/nats-operator");
    let rendered = helm_template_render_rbac(&chart_dir, false);
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn helm_template_renders_successfully_cluster_scoped() {
    let chart_dir = test_util::workspace_testdata().join("charts/nats-operator");
    let rendered = helm_template_render_rbac(&chart_dir, true);
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

fn validate_rendered_docs(rendered_yaml: &str) {
    let docs = parse_yaml_documents(rendered_yaml);
    assert!(!docs.is_empty(), "rendered YAML contained no documents");

    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    for doc in docs {
        let api_version = doc
            .get("apiVersion")
            .and_then(|v| v.as_str())
            .expect("manifest missing apiVersion");
        let kind = doc
            .get("kind")
            .and_then(|v| v.as_str())
            .expect("manifest missing kind");

        let resource = ResourceRef {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };

        if let Some(schema) = debug_materialize_schema_for_resource(&provider, &resource) {
            let schema = match schema {
                serde_json::Value::Object(mut obj) => {
                    let _ = obj.remove("$schema");
                    serde_json::Value::Object(obj)
                }
                other => other,
            };

            let errors = common::validate_json_against_schema(&doc, &schema);
            assert!(
                errors.is_empty(),
                "rendered {api_version}/{kind} failed upstream K8s schema validation with {} error(s):\n{}",
                errors.len(),
                errors.join("\n")
            );
        } else {
            panic!("load upstream k8s schema for {api_version}/{kind}");
        }
    }
}

#[test]
fn rendered_rbac_validates_against_upstream_k8s_schema_default_values() {
    let chart_dir = test_util::workspace_testdata().join("charts/nats-operator");
    let rendered_yaml = helm_template_render_rbac(&chart_dir, false).expect("helm template");
    validate_rendered_docs(&rendered_yaml);
}

#[test]
fn rendered_rbac_validates_against_upstream_k8s_schema_cluster_scoped() {
    let chart_dir = test_util::workspace_testdata().join("charts/nats-operator");
    let rendered_yaml = helm_template_render_rbac(&chart_dir, true).expect("helm template");
    validate_rendered_docs(&rendered_yaml);
}
