#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

const TEMPLATE_PATH: &str =
    "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml";
const HELPERS_PATH: &str =
    "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/_helpers.tpl";
const COMMON_TEMPLATES_DIR: &str =
    "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/charts/common/templates";

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(parser, &test_util::read_testdata(HELPERS_PATH))
        .expect("helpers");
    for src in test_util::read_testdata_dir(COMMON_TEMPLATES_DIR, "tpl") {
        let _ = idx.add_source(parser, &src);
    }
    idx
}

#[test]
fn resource_detection() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "v1".to_string(),
            kind: "Secret".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let _secret = serde_json::json!({"api_version": "v1", "kind": "Secret"});
    let _t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let _eq_repl =
        serde_json::json!({"type": "eq", "path": "architecture", "value": "replication"});

    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/signoz_postgresql_secrets.ir.json"))
            .expect("expected ir json");

    similar_asserts::assert_eq!(actual, expected);
}
