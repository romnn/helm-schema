#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};

const TEMPLATE_PATH: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml";
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
fn symbolic_ir_full() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual = serde_json::to_value(&ir).unwrap();

    if std::env::var("IR_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/signoz_zookeeper_statefulset.ir.json"
    ))
    .expect("expected ir json");

    similar_asserts::assert_eq!(actual, expected);
}
