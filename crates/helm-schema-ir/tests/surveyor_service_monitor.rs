#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

const TEMPLATE_PATH: &str = "charts/surveyor/templates/serviceMonitor.yaml";

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/surveyor/templates/_helpers.tpl"),
    )
    .expect("helpers");
    idx
}

#[test]
fn resource_detection() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "monitoring.coreos.com/v1".to_string(),
            kind: "ServiceMonitor".to_string(),
            api_version_candidates: Vec::new(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
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

    let sm =
        serde_json::json!({"api_version": "monitoring.coreos.com/v1", "kind": "ServiceMonitor"});
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
            "source_expr": "serviceMonitor.annotations",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceMonitor.enabled")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.annotations",
            "path": ["metadata", "annotations"],
            "kind": "Fragment",
            "guards": [t("serviceMonitor.enabled"), t("serviceMonitor.annotations")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "serviceMonitor.interval",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceMonitor.enabled")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.interval",
            "path": ["spec", "endpoints[*]", "interval"],
            "kind": "Scalar",
            "guards": [t("serviceMonitor.enabled"), t("serviceMonitor.interval")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.labels",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceMonitor.enabled")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.labels",
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": [t("serviceMonitor.enabled"), t("serviceMonitor.labels")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.metricRelabelings",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceMonitor.enabled")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.metricRelabelings",
            "path": ["spec", "endpoints[*]", "metricRelabelings"],
            "kind": "Fragment",
            "guards": [t("serviceMonitor.enabled"), t("serviceMonitor.metricRelabelings")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.relabelings",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceMonitor.enabled")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.relabelings",
            "path": ["spec", "endpoints[*]", "relabelings"],
            "kind": "Fragment",
            "guards": [t("serviceMonitor.enabled"), t("serviceMonitor.relabelings")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.scrapeTimeout",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceMonitor.enabled")],
            "resource": sm
        },
        {
            "source_expr": "serviceMonitor.scrapeTimeout",
            "path": ["spec", "endpoints[*]", "scrapeTimeout"],
            "kind": "Scalar",
            "guards": [t("serviceMonitor.enabled"), t("serviceMonitor.scrapeTimeout")],
            "resource": sm
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
