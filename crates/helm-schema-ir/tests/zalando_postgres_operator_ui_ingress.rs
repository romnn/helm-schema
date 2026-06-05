#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

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
fn resource_detection() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator-ui/templates/ingress.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "networking.k8s.io/v1".to_string(),
            kind: "Ingress".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
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
    let ingress = serde_json::json!({
        "api_version": "networking.k8s.io/v1",
        "kind": "Ingress",
        "api_version_candidates": [
            "networking.k8s.io/v1beta1",
            "extensions/v1beta1"
        ]
    });
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let r = |p: &str| serde_json::json!({"type": "range", "path": p});
    let w = |p: &str| serde_json::json!({"type": "with", "path": p});

    let expected = serde_json::json!([
        {
            "source_expr": "fullnameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "ingress.annotations",
            "path": [],
            "kind": "Scalar",
            "guards": [t("ingress.enabled"), w("ingress.annotations")],
            "resource": ingress
        },
        {
            "source_expr": "ingress.annotations",
            "path": ["metadata", "annotations"],
            "kind": "Fragment",
            "guards": [t("ingress.enabled"), w("ingress.annotations")],
            "resource": ingress
        },
        {
            "source_expr": "ingress.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "ingress.hosts",
            "path": ["spec", "rules"],
            "kind": "Scalar",
            "guards": [t("ingress.enabled")],
            "resource": ingress
        },
        {
            "source_expr": "ingress.hosts.*.host",
            "path": ["spec", "rules[*]", "host"],
            "kind": "Scalar",
            "guards": [t("ingress.enabled"), r("ingress.hosts")],
            "resource": ingress
        },
        {
            "source_expr": "ingress.ingressClassName",
            "path": [],
            "kind": "Scalar",
            "guards": [t("ingress.enabled")],
            "resource": ingress
        },
        {
            "source_expr": "ingress.ingressClassName",
            "path": ["spec", "ingressClassName"],
            "kind": "Scalar",
            "guards": [t("ingress.enabled"), t("ingress.ingressClassName")],
            "resource": ingress
        },
        {
            "source_expr": "ingress.tls",
            "path": [],
            "kind": "Scalar",
            "guards": [t("ingress.enabled")],
            "resource": ingress
        },
        {
            "source_expr": "ingress.tls",
            "path": ["spec", "tls"],
            "kind": "Scalar",
            "guards": [t("ingress.enabled"), t("ingress.tls")],
            "resource": ingress
        },
        {
            "source_expr": "ingress.tls.*.hosts",
            "path": ["spec", "tls[*]", "hosts"],
            "kind": "Scalar",
            "guards": [t("ingress.enabled"), t("ingress.tls"), r("ingress.tls")],
            "resource": ingress
        },
        {
            "source_expr": "ingress.tls.*.hosts.*",
            "path": ["spec", "tls[*]", "hosts[*]"],
            "kind": "Scalar",
            "guards": [t("ingress.enabled"), t("ingress.tls"), r("ingress.tls"), r("ingress.tls.*.hosts")],
            "resource": ingress
        },
        {
            "source_expr": "nameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "service.port",
            "path": [],
            "kind": "Scalar",
            "guards": [t("ingress.enabled")],
            "resource": null
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
