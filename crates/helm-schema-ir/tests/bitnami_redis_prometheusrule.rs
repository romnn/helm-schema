mod common;

use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

/// `DefaultResourceDetector` finds the `PrometheusRule` resource type.
#[test]
fn resource_detection() {
    let src = common::prometheusrule_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "monitoring.coreos.com/v1".to_string(),
            kind: "PrometheusRule".to_string(),
        })
    );
}

#[test]
fn symbolic_ir_full() {
    let src = common::prometheusrule_src();
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = common::build_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let pr =
        serde_json::json!({"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"});

    let expected = serde_json::json!([
        {
            "source_expr": "commonAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": [t("metrics.enabled"), t("metrics.prometheusRule.enabled")],
            "resource": pr
        },
        {
            "source_expr": "commonAnnotations",
            "path": ["metadata", "annotations"],
            "kind": "Fragment",
            "guards": [t("metrics.enabled"), t("metrics.prometheusRule.enabled"), t("commonAnnotations")],
            "resource": pr
        },
        {
            "source_expr": "commonLabels",
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": [t("metrics.enabled"), t("metrics.prometheusRule.enabled")],
            "resource": pr
        },
        {
            "source_expr": "metrics.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "metrics.prometheusRule.additionalLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [t("metrics.enabled"), t("metrics.prometheusRule.enabled")],
            "resource": pr
        },
        {
            "source_expr": "metrics.prometheusRule.additionalLabels",
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": [t("metrics.enabled"), t("metrics.prometheusRule.enabled"), t("metrics.prometheusRule.additionalLabels")],
            "resource": pr
        },
        {
            "source_expr": "metrics.prometheusRule.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("metrics.enabled")],
            "resource": null
        },
        {
            "source_expr": "metrics.prometheusRule.namespace",
            "path": ["metadata", "namespace"],
            "kind": "Scalar",
            "guards": [t("metrics.enabled"), t("metrics.prometheusRule.enabled")],
            "resource": pr
        },
        {
            "source_expr": "metrics.prometheusRule.rules",
            "path": ["spec", "groups[*]", "rules"],
            "kind": "Fragment",
            "guards": [t("metrics.enabled"), t("metrics.prometheusRule.enabled")],
            "resource": pr
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
