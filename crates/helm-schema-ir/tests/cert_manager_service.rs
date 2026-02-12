#![recursion_limit = "1024"]

mod common;

use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

#[test]
fn resource_detection() {
    let src = common::cert_manager_service_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "v1".to_string(),
            kind: "Service".to_string(),
        })
    );
}

#[test]
fn symbolic_ir_full() {
    let src = common::cert_manager_service_src();
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = common::build_cert_manager_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let svc = serde_json::json!({"api_version": "v1", "kind": "Service"});
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    let expected = serde_json::json!([
        {
            "source_expr": "prometheus.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "prometheus.podmonitor.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("prometheus.enabled")],
            "resource": null
        },
        {
            "source_expr": "prometheus.servicemonitor.targetPort",
            "path": ["spec", "ports[*]", "targetPort"],
            "kind": "Scalar",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled")],
            "resource": svc
        },
        {
            "source_expr": "serviceAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled")],
            "resource": svc
        },
        {
            "source_expr": "serviceAnnotations",
            "path": ["metadata", "annotations"],
            "kind": "Fragment",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled"), t("serviceAnnotations")],
            "resource": svc
        },
        {
            "source_expr": "serviceIPFamilies",
            "path": [],
            "kind": "Scalar",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled")],
            "resource": svc
        },
        {
            "source_expr": "serviceIPFamilies",
            "path": ["spec", "ipFamilies"],
            "kind": "Fragment",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled"), t("serviceIPFamilies")],
            "resource": svc
        },
        {
            "source_expr": "serviceIPFamilyPolicy",
            "path": [],
            "kind": "Scalar",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled")],
            "resource": svc
        },
        {
            "source_expr": "serviceIPFamilyPolicy",
            "path": ["spec", "ipFamilyPolicy"],
            "kind": "Scalar",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled"), t("serviceIPFamilyPolicy")],
            "resource": svc
        },
        {
            "source_expr": "serviceLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled")],
            "resource": svc
        },
        {
            "source_expr": "serviceLabels",
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled"), t("serviceLabels")],
            "resource": svc
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
