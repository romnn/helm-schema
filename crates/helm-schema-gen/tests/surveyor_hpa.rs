#![recursion_limit = "512"]

mod common;

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, ResourceRef, SymbolicIrGenerator};
use helm_schema_k8s::{UpstreamK8sSchemaProvider, WarningSink};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

const TEMPLATE_PATH: &str = "charts/surveyor/templates/hpa.yaml";
const VALUES_PATH: &str = "charts/surveyor/values.yaml";

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/surveyor/templates/_helpers.tpl"),
    )
    .expect("helpers");
    idx
}

#[test]
fn warns_when_hpa_v2beta1_schema_missing_in_newer_k8s_bundle() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let warnings: WarningSink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    let provider = UpstreamK8sSchemaProvider::new("v1.35.0")
        .with_cache_dir(test_util::workspace_testdata().join("kubernetes-json-schema"))
        .with_allow_download(false)
        .with_warning_sink(warnings.clone());

    let _schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual = warnings.lock().expect("warnings lock").clone();
    let w = actual
        .iter()
        .find(|w| {
            w.kind == "HorizontalPodAutoscaler"
                && w.api_version == "autoscaling/v2beta1"
                && w.k8s_version == "v1.35.0"
        })
        .unwrap_or_else(|| {
            panic!(
                "expected a missing-upstream-schema warning for HPA autoscaling/v2beta1 in v1.35.0; got: {actual:?}"
            )
        });

    assert!(
        w.hint
            .as_deref()
            .is_some_and(|h| h.contains("removed in Kubernetes v1.25+")),
        "expected warning hint to mention removal in Kubernetes v1.25+; got: {w:?}"
    );
}

fn helm_template_render_hpa(chart_dir: &Path) -> Result<String, String> {
    let mut cmd = Command::new("helm");
    cmd.arg("template")
        .arg("test-release")
        .arg(chart_dir)
        .arg("--show-only")
        .arg("templates/hpa.yaml")
        .arg("--set")
        .arg("autoscaling.enabled=true")
        .arg("--kube-version")
        .arg("1.24.0");

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
#[allow(clippy::too_many_lines)]
fn schema_fused_rust() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    // The Surveyor chart template hardcodes autoscaling/v2beta1, and it also uses
    // v2beta1-only metric fields like `targetAverageUtilization`.
    //
    // Upstream Kubernetes removed autoscaling/v2beta1 for HorizontalPodAutoscaler in newer
    // releases, so we must validate/generate against an upstream schema bundle that still
    // contains that apiVersion.
    let provider = UpstreamK8sSchemaProvider::new("v1.24.0").with_allow_download(true);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/surveyor_hpa.schema.json"))
            .expect("expected schema json");

    similar_asserts::assert_eq!(actual, expected);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/surveyor");
    let rendered = helm_template_render_hpa(&chart_dir);
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
    // See comment in `schema_fused_rust`.
    let provider = UpstreamK8sSchemaProvider::new("v1.24.0").with_allow_download(true);
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
fn rendered_hpa_validates_against_upstream_k8s_schema() {
    let chart_dir = test_util::workspace_testdata().join("charts/surveyor");

    let rendered_yaml = helm_template_render_hpa(&chart_dir).expect("helm template");
    let docs = parse_yaml_documents(&rendered_yaml);
    assert!(!docs.is_empty(), "rendered YAML contained no documents");

    let hpa_doc = docs
        .into_iter()
        .find(|d| d.get("kind").and_then(|v| v.as_str()) == Some("HorizontalPodAutoscaler"))
        .expect("rendered output did not contain a HorizontalPodAutoscaler document");

    let api_version = hpa_doc
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .expect("HPA manifest missing apiVersion");
    let kind = hpa_doc
        .get("kind")
        .and_then(|v| v.as_str())
        .expect("HPA manifest missing kind");

    let resource = ResourceRef {
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        api_version_candidates: Vec::new(),
    };

    // The Surveyor chart template hardcodes autoscaling/v2beta1, which was removed from newer
    // Kubernetes releases, so we validate it against an upstream schema bundle where
    // autoscaling/v2beta1 is still present.
    let provider = UpstreamK8sSchemaProvider::new("v1.24.0").with_allow_download(true);
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

    let errors = common::validate_json_against_schema(&hpa_doc, &schema);
    assert!(
        errors.is_empty(),
        "rendered HPA failed upstream K8s schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
