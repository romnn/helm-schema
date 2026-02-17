#![recursion_limit = "512"]

mod common;

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::KubernetesJsonSchemaProvider;

const TEMPLATE_PATH: &str = "charts/nats/templates/service-account.yaml";
const VALUES_PATH: &str = "charts/nats/values.yaml";

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();

    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_helpers.tpl"),
    );
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_jsonpatch.tpl"),
    );
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_tplYaml.tpl"),
    );
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_toPrettyRawJson.tpl"),
    );

    idx.add_file_source(
        "files/service-account.yaml",
        &test_util::read_testdata("charts/nats/files/service-account.yaml"),
    );

    idx
}

#[test]
#[allow(clippy::too_many_lines)]
fn schema_fused_rust() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
        let path = std::env::temp_dir().join("helm-schema.nats-service-account.schema.json");
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&actual).expect("json bytes"),
        )
        .expect("write schema dump");
    }

    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/nats_service_account.schema.json"))
            .expect("expected schema json");

    similar_asserts::assert_eq!(actual, expected);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/nats");

    // This template is gated by `.Values.serviceAccount.enabled`, which is `false` by default.
    // Enable it so we assert the rendered output path actually works.
    let output = std::process::Command::new("helm")
        .arg("template")
        .arg("test-release")
        .arg(&chart_dir)
        .arg("--show-only")
        .arg("templates/service-account.yaml")
        .arg("--set")
        .arg("serviceAccount.enabled=true")
        .output()
        .expect("run helm template");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!("helm template failed:\nstderr: {stderr}\nstdout: {stdout}");
    }

    let yaml = String::from_utf8(output.stdout).expect("helm output utf-8");
    assert!(!yaml.is_empty(), "rendered YAML is empty");
}

#[test]
fn schema_validates_values_yaml() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let errors = common::validate_values_yaml(&values_yaml, &schema);
    assert!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
