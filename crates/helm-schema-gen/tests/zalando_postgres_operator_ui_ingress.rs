#![recursion_limit = "512"]

mod common;

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, ResourceRef, SymbolicIrGenerator};
use helm_schema_k8s::UpstreamK8sSchemaProvider;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

const TEMPLATE_PATH: &str = "charts/zalando-postgres-operator-ui/templates/ingress.yaml";
const VALUES_PATH: &str = "charts/zalando-postgres-operator-ui/values.yaml";

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/zalando-postgres-operator-ui/templates/_helpers.tpl"),
    );
    idx
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

fn helm_template_render_ingress(chart_dir: &Path) -> Result<String, String> {
    let mut cmd = Command::new("helm");
    cmd.arg("template")
        .arg("test-release")
        .arg(chart_dir)
        .arg("--show-only")
        .arg("templates/ingress.yaml")
        .arg("--set")
        .arg("ingress.enabled=true")
        .arg("--kube-version")
        .arg("1.29.0");

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run helm: {e}"))?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|e| format!("helm output is not valid UTF-8: {e}"))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "helm template failed:\nstderr: {stderr}\nstdout: {stdout}"
        ))
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn schema_fused_rust() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
        let path = std::env::temp_dir()
            .join("helm-schema.zalando-postgres-operator-ui.ingress.schema.json");
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&actual).expect("json bytes"),
        )
        .expect("write schema dump");
    }

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/zalando_postgres_operator_ui_ingress.schema.json"
    ))
    .expect("expected schema json");

    similar_asserts::assert_eq!(actual, expected);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/zalando-postgres-operator-ui");
    let rendered = helm_template_render_ingress(&chart_dir);
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn schema_validates_values_yaml() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let errors = common::validate_values_yaml(&values_yaml, &schema);
    assert!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}

#[test]
fn rendered_ingress_validates_against_upstream_k8s_schema() {
    let chart_dir = test_util::workspace_testdata().join("charts/zalando-postgres-operator-ui");
    let rendered_yaml = helm_template_render_ingress(&chart_dir).expect("helm template");
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
    };

    let provider = UpstreamK8sSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = provider
        .materialize_schema_for_resource(&resource)
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
