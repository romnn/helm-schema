#![recursion_limit = "512"]

mod common;

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::SymbolicIrContext;

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/bitnami-redis/templates/_helpers.tpl"),
    );
    for src in test_util::read_testdata_dir("charts/common/templates", "tpl") {
        let _ = idx.add_source(parser, &src);
    }
    idx
}

/// Full schema generation for networkpolicy using tree-sitter parser.
///
/// The generated schema should capture all `.Values.*` references from the
/// networkpolicy template and produce a well-structured JSON schema that a
/// devops engineer would recognize as describing the values.yaml structure.
#[test]
#[allow(clippy::too_many_lines)]
fn schema_from_tree_sitter() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/networkpolicy.yaml");
    let values_yaml = test_util::read_testdata("charts/bitnami-redis/values.yaml");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrContext::new(&idx).generate_contract_ir(&src, &idx);
    // This chart's `apiVersion` comes from a helper
    // (`common.capabilities.networkPolicy.apiVersion`). A bare K8s provider
    // no longer resolves empty `api_version`; the chain's inference path is
    // the intended route for recovering `networking.k8s.io/v1` from
    // `kind: NetworkPolicy`.
    let provider = common::production_k8s_chain("v1.35.0");
    let schema = common::generate_schema_with_values_yaml(ir, &provider, Some(&values_yaml));

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&schema).expect("pretty json")
        );
        let path = std::env::temp_dir().join("helm-schema.bitnami-redis.networkpolicy.schema.json");
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&schema).expect("json bytes"),
        )
        .expect("write schema dump");
    }
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/bitnami_redis_networkpolicy.schema.json"
    ))
    .expect("expected schema json");

    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn schema_validates_values_yaml() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/networkpolicy.yaml");
    let values_yaml = test_util::read_testdata("charts/bitnami-redis/values.yaml");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrContext::new(&idx).generate_contract_ir(&src, &idx);
    let provider = common::production_k8s_chain("v1.35.0");
    let schema = common::generate_schema_with_values_yaml(ir, &provider, Some(&values_yaml));

    let errors = common::validate_values_yaml(&values_yaml, &schema);
    assert!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
