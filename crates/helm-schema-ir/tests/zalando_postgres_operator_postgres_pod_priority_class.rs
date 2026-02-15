#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/zalando-postgres-operator/templates/_helpers.tpl"),
    )
    .expect("helpers");
    idx
}

#[test]
fn resource_detection() {
    let src = test_util::read_testdata(
        "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
    );
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "scheduling.k8s.io/v1".to_string(),
            kind: "PriorityClass".to_string(),
            api_version_candidates: Vec::new(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src = test_util::read_testdata(
        "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
    );
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

    let pc = serde_json::json!({
        "api_version": "scheduling.k8s.io/v1",
        "kind": "PriorityClass"
    });
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    let expected = serde_json::json!([
        {
            "source_expr": "nameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "podPriorityClassName.create",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "podPriorityClassName.name",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "podPriorityClassName.priority",
            "path": ["value"],
            "kind": "Scalar",
            "guards": [t("podPriorityClassName.create")],
            "resource": pc
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
