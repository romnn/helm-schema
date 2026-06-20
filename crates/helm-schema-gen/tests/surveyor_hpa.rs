#![recursion_limit = "512"]

mod common;

use helm_schema_ir::ResourceRef;
use helm_schema_k8s::{
    Chain, Diagnostic, DiagnosticSink, KubernetesJsonSchemaProvider,
    kubernetes_openapi::debug_materialize_schema_for_resource,
};
use serde::Deserialize;

use common::cases::SURVEYOR_HPA as CASE;

#[test]
fn warns_when_hpa_v2beta1_schema_missing_in_newer_k8s_bundle() {
    let src = test_util::read_testdata(CASE.template_path);
    let values_yaml = test_util::read_testdata(CASE.values_path);
    let idx = common::build_define_index(
        &helm_schema_ast::TreeSitterParser,
        CASE.define_sources,
        CASE.helper_parse_mode,
    );
    let ir = helm_schema_ir::SymbolicIrContext::new(&idx).generate_contract_ir(&src, &idx);

    let diagnostics = DiagnosticSink::new();
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_allow_download(true)
        .with_diagnostic_sink(diagnostics.clone());
    let chain = Chain::new(vec![Box::new(k8s_provider)]).with_diagnostic_sink(diagnostics.clone());

    let _schema = common::generate_schema_with_values_yaml(ir, &chain, Some(&values_yaml));

    let actual = diagnostics.snapshot();
    let w = actual
        .iter()
        .find_map(|d| match d {
            Diagnostic::MissingSchema {
                kind,
                api_version,
                k8s_versions_tried,
                hint,
                ..
            } if kind == "HorizontalPodAutoscaler"
                && api_version == "autoscaling/v2beta1"
                && k8s_versions_tried.iter().any(|v| v == "v1.35.0") =>
            {
                Some(hint.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "expected a missing-upstream-schema warning for HPA autoscaling/v2beta1 in v1.35.0; got: {actual:?}"
            )
        });

    assert!(
        w.as_deref()
            .is_some_and(|h| h.contains("removed in Kubernetes v1.25+")),
        "expected warning hint to mention removal in Kubernetes v1.25+; got: {w:?}"
    );
}

fn helm_template_render_hpa(chart_dir: &std::path::Path) -> Result<String, String> {
    common::helm_template_render_with_args(
        chart_dir,
        Some("templates/hpa.yaml"),
        &[
            "--set",
            "autoscaling.enabled=true",
            "--kube-version",
            "1.24.0",
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
        api_version_branches: Vec::new(),
    };

    let provider = KubernetesJsonSchemaProvider::new("v1.24.0").with_allow_download(true);
    let schema = debug_materialize_schema_for_resource(&provider, &resource)
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
