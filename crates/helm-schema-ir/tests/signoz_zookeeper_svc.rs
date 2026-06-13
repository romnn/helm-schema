#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::SymbolicIrGenerator;

const TEMPLATE_PATH: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml";
const HELPERS_PATH: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/_helpers.tpl";
const COMMON_TEMPLATES_DIR: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates";

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
#[allow(clippy::too_many_lines)]
fn symbolic_ir_from_tree_sitter() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("IR_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let _svc = serde_json::json!({"api_version": "v1", "kind": "Service"});
    let _t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let _n = |p: &str| serde_json::json!({"type": "not", "path": p});
    let _o = |a: &str, b: &str| serde_json::json!({"type": "or", "paths": [a, b]});

    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/signoz_zookeeper_svc.ir.json"))
            .expect("expected ir json");

    similar_asserts::assert_eq!(actual, expected);
}
