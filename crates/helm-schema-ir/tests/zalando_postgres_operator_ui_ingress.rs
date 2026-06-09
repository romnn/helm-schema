#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/zalando-postgres-operator-ui/templates/_helpers.tpl"),
    )
    .expect("helpers");
    idx
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_from_tree_sitter() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator-ui/templates/ingress.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    // Source-order, not alphabetical: the chart's `if/else if/else`
    // chain declares them in this sequence (primary → v1beta1 fallback
    // → legacy extensions fallback). The detector preserves that
    // order verbatim instead of imposing a generic stability rank
    // (round-5 Finding 2 fix).
    let _ingress = serde_json::json!({
        "api_version": "networking.k8s.io/v1",
        "kind": "Ingress",
        "api_version_candidates": [
            "networking.k8s.io/v1beta1",
            "extensions/v1beta1"
        ]
    });
    let _t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let _r = |p: &str| serde_json::json!({"type": "range", "path": p});
    let _w = |p: &str| serde_json::json!({"type": "with", "path": p});

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/zalando_postgres_operator_ui_ingress.ir.json"
    ))
    .expect("expected ir json");

    similar_asserts::assert_eq!(actual, expected);
}
