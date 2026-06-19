#![recursion_limit = "512"]

mod common;

use helm_schema_ir::ResourceRef;
use helm_schema_k8s::{
    CrdsCatalogSchemaProvider, crds_catalog::debug_materialize_schema_for_resource,
};
use serde::Deserialize;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/surveyor/templates/serviceMonitor.yaml",
    values_path: "charts/surveyor/values.yaml",
    expected_fixture: include_str!("fixtures/surveyor_service_monitor.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: common::ProviderKind::CrdK8s("v1.35.0"),
    dump_stem: "surveyor.service-monitor",
};

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

fn helm_template_render_service_monitor(chart_dir: &std::path::Path) -> Result<String, String> {
    common::helm_template_render_with_args(
        chart_dir,
        Some("templates/serviceMonitor.yaml"),
        &[
            "--set",
            "serviceMonitor.enabled=true",
            "--kube-version",
            "1.29.0",
        ],
    )
}

#[test]
fn schema_from_tree_sitter() {
    let actual = common::render_schema_case_strict_helpers(&CASE);
    let expected: serde_json::Value =
        serde_json::from_str(CASE.expected_fixture).expect("expected schema json");
    similar_asserts::assert_eq!(actual, expected);

    let values_yaml = test_util::read_testdata(CASE.values_path);
    let errors = common::validate_values_yaml(&values_yaml, &actual);
    assert!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/surveyor");
    let rendered = helm_template_render_service_monitor(&chart_dir);
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn rendered_service_monitor_validates_against_crd_schema() {
    let chart_dir = test_util::workspace_testdata().join("charts/surveyor");
    let rendered_yaml = helm_template_render_service_monitor(&chart_dir).expect("helm template");
    let docs = parse_yaml_documents(&rendered_yaml);
    assert!(!docs.is_empty(), "rendered YAML contained no documents");

    let sm_doc = docs
        .into_iter()
        .find(|d| d.get("kind").and_then(|v| v.as_str()) == Some("ServiceMonitor"))
        .expect("rendered output did not contain a ServiceMonitor document");

    let api_version = sm_doc
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .expect("ServiceMonitor manifest missing apiVersion");
    let kind = sm_doc
        .get("kind")
        .and_then(|v| v.as_str())
        .expect("ServiceMonitor manifest missing kind");

    let resource = ResourceRef {
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);
    let schema = debug_materialize_schema_for_resource(&provider, &resource)
        .expect("load crd schema for rendered resource");

    let schema = match schema {
        serde_json::Value::Object(mut obj) => {
            let _ = obj.remove("$schema");
            serde_json::Value::Object(obj)
        }
        other => other,
    };

    let errors = common::validate_json_against_schema(&sm_doc, &schema);
    assert!(
        errors.is_empty(),
        "rendered ServiceMonitor failed CRD schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
