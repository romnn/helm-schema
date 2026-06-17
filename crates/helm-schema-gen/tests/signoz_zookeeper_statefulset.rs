#![recursion_limit = "512"]

mod common;

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::SymbolicIrContext;

const TEMPLATE_PATH: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml";
const VALUES_PATH: &str = "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/values.yaml";
const HELPERS_PATH: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/_helpers.tpl";
const COMMON_TEMPLATES_DIR: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates";

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(parser, &test_util::read_testdata(HELPERS_PATH));
    for src in test_util::read_testdata_dir(COMMON_TEMPLATES_DIR, "tpl") {
        let _ = idx.add_source(parser, &src);
    }
    idx
}

#[test]
#[allow(clippy::too_many_lines)]
fn schema_from_tree_sitter() {
    // Keep this body touched when fixture snapshots change so incremental rebuilds
    // definitely refresh the embedded expected schema.
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrContext::new(&idx).generate_contract_ir(&src, &idx);
    if std::env::var("IR_DUMP").is_ok() {
        let projection = ir.clone().project();
        eprintln!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::to_value(helm_schema_ir::ContractDocumentV1::from_projection(
                    projection,
                ))
                .expect("ir json"),
            )
            .expect("pretty ir")
        );
    }
    let provider = common::production_k8s_chain("v1.35.0");
    let schema = common::generate_schema_with_values_yaml(ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
        let path =
            std::env::temp_dir().join("helm-schema.signoz-zookeeper-statefulset.schema.json");
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&actual).expect("json bytes"),
        )
        .expect("write schema dump");
    }

    let expected_json: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/signoz_zookeeper_statefulset.schema.json"
    ))
    .expect("expected schema json");

    similar_asserts::assert_eq!(actual, expected_json);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata()
        .join("charts/signoz-signoz/charts/clickhouse/charts/zookeeper");
    let rendered = common::helm_template_render(&chart_dir, Some("templates/statefulset.yaml"));
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn schema_validates_values_yaml() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
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
