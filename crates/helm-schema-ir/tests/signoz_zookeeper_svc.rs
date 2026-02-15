#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

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
fn resource_detection() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
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
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("IR_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let svc = serde_json::json!({"api_version": "v1", "kind": "Service"});
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let n = |p: &str| serde_json::json!({"type": "not", "path": p});
    let o = |a: &str, b: &str| serde_json::json!({"type": "or", "paths": [a, b]});

    let expected = serde_json::json!([
        {
            "source_expr": "commonAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "commonAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": [o("commonAnnotations", "service.annotations")],
            "resource": svc
        },
        {
            "source_expr": "commonAnnotations",
            "path": ["metadata", "annotations"],
            "kind": "Fragment",
            "guards": [o("commonAnnotations", "service.annotations"), t("commonAnnotations")],
            "resource": svc
        },
        {
            "source_expr": "commonLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "commonLabels",
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": [t("commonLabels")],
            "resource": svc
        },
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
            "source_expr": "namespaceOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "service.annotations",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.annotations",
            "path": [],
            "kind": "Scalar",
            "guards": [o("commonAnnotations", "service.annotations")],
            "resource": svc
        },
        {
            "source_expr": "service.annotations",
            "path": ["metadata", "annotations"],
            "kind": "Fragment",
            "guards": [o("commonAnnotations", "service.annotations"), t("service.annotations")],
            "resource": svc
        },
        {
            "source_expr": "service.clusterIP",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.clusterIP",
            "path": ["spec", "clusterIP"],
            "kind": "Scalar",
            "guards": [t("service.clusterIP"), t("service.type")],
            "resource": svc
        },
        {
            "source_expr": "service.disableBaseClientPort",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.externalTrafficPolicy",
            "path": ["spec", "externalTrafficPolicy"],
            "kind": "Scalar",
            "guards": [t("service.type")],
            "resource": svc
        },
        {
            "source_expr": "service.extraPorts",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.extraPorts",
            "path": ["spec", "ports"],
            "kind": "Fragment",
            "guards": [t("service.extraPorts")],
            "resource": svc
        },
        {
            "source_expr": "service.loadBalancerIP",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.loadBalancerIP",
            "path": ["spec", "loadBalancerIP"],
            "kind": "Scalar",
            "guards": [t("service.loadBalancerIP"), t("service.type")],
            "resource": svc
        },
        {
            "source_expr": "service.loadBalancerSourceRanges",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.loadBalancerSourceRanges",
            "path": ["spec", "loadBalancerSourceRanges"],
            "kind": "Scalar",
            "guards": [t("service.loadBalancerSourceRanges"), t("service.type")],
            "resource": svc
        },
        {
            "source_expr": "service.nodePorts.client",
            "path": [],
            "kind": "Scalar",
            "guards": [n("service.disableBaseClientPort")],
            "resource": svc
        },
        {
            "source_expr": "service.nodePorts.client",
            "path": ["spec", "ports[*]", "nodePort"],
            "kind": "Scalar",
            "guards": [
                n("service.disableBaseClientPort"),
                t("service.nodePorts.client"),
                t("service.type")
            ],
            "resource": svc
        },
        {
            "source_expr": "service.nodePorts.tls",
            "path": [],
            "kind": "Scalar",
            "guards": [t("tls.client.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.nodePorts.tls",
            "path": ["spec", "ports[*]", "nodePort"],
            "kind": "Scalar",
            "guards": [t("tls.client.enabled"), t("service.nodePorts.tls"), t("service.type")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.client",
            "path": ["spec", "ports[*]", "port"],
            "kind": "Scalar",
            "guards": [n("service.disableBaseClientPort")],
            "resource": svc
        },
        {
            "source_expr": "service.ports.election",
            "path": ["spec", "ports[*]", "port"],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.ports.follower",
            "path": ["spec", "ports[*]", "port"],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.ports.tls",
            "path": ["spec", "ports[*]", "port"],
            "kind": "Scalar",
            "guards": [t("tls.client.enabled")],
            "resource": svc
        },
        {
            "source_expr": "service.sessionAffinity",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.sessionAffinity",
            "path": ["spec", "sessionAffinity"],
            "kind": "Scalar",
            "guards": [t("service.sessionAffinity")],
            "resource": svc
        },
        {
            "source_expr": "service.sessionAffinityConfig",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.sessionAffinityConfig",
            "path": ["spec", "sessionAffinityConfig"],
            "kind": "Fragment",
            "guards": [t("service.sessionAffinityConfig")],
            "resource": svc
        },
        {
            "source_expr": "service.type",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "service.type",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service.clusterIP")],
            "resource": svc
        },
        {
            "source_expr": "service.type",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service.loadBalancerIP")],
            "resource": svc
        },
        {
            "source_expr": "service.type",
            "path": [],
            "kind": "Scalar",
            "guards": [t("service.loadBalancerSourceRanges")],
            "resource": svc
        },
        {
            "source_expr": "service.type",
            "path": [],
            "kind": "Scalar",
            "guards": [t("tls.client.enabled"), t("service.nodePorts.tls")],
            "resource": svc
        },
        {
            "source_expr": "service.type",
            "path": [],
            "kind": "Scalar",
            "guards": [n("service.disableBaseClientPort"), t("service.nodePorts.client")],
            "resource": svc
        },
        {
            "source_expr": "service.type",
            "path": ["spec", "type"],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        },
        {
            "source_expr": "tls.client.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": svc
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
