#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

fn build_cert_manager_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/cert-manager/templates/_helpers.tpl"),
    )
    .expect("cert-manager helpers");
    idx
}

#[test]
fn resource_detection() {
    let src = test_util::read_testdata("charts/cert-manager/templates/service.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "v1".to_string(),
            kind: "Service".to_string(),
            api_version_candidates: Vec::new(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src = test_util::read_testdata("charts/cert-manager/templates/service.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_cert_manager_define_index(&TreeSitterParser);
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
            "source_expr": "fullnameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "nameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "namespace",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
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
