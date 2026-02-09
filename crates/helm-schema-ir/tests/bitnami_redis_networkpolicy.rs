mod common;

use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultIrGenerator, DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef,
};

/// Resource detection for networkpolicy (kind is scalar, apiVersion is template).
#[test]
fn resource_detection() {
    let src = common::networkpolicy_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    // apiVersion is a template call, not a scalar, so it won't be detected.
    // kind: NetworkPolicy IS a scalar.
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: String::new(),
            kind: "NetworkPolicy".to_string(),
        })
    );
}

/// Both parsers produce same IR for networkpolicy.
#[test]
fn both_parsers_produce_same_ir() {
    let src = common::networkpolicy_src();

    let rust_ast = FusedRustParser.parse(&src).expect("fused rust");
    let rust_idx = common::build_define_index(&FusedRustParser);
    let rust_ir = DefaultIrGenerator.generate(&rust_ast, &rust_idx);

    let ts_ast = TreeSitterParser.parse(&src).expect("tree-sitter");
    let ts_idx = common::build_define_index(&TreeSitterParser);
    let ts_ir = DefaultIrGenerator.generate(&ts_ast, &ts_idx);

    let rust_json: serde_json::Value = serde_json::to_value(&rust_ir).expect("serialize");
    let ts_json: serde_json::Value = serde_json::to_value(&ts_ir).expect("serialize");
    similar_asserts::assert_eq!(rust_json, ts_json);
}

/// Full hardcoded expected IR for networkpolicy using fused-Rust parser.
///
/// Uses structured Guard types: Truthy, Not, Or, Eq.
/// Guards are deduplicated (no duplicate from if→range chains).
/// `not` conditions produce Guard::Not, `or` produces Guard::Or, `eq` produces Guard::Eq.
#[test]
fn fused_rust_ir_full() {
    let src = common::networkpolicy_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = common::build_define_index(&FusedRustParser);
    let ir = DefaultIrGenerator.generate(&ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    let np = serde_json::json!({"api_version": "", "kind": "NetworkPolicy"});
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let n = |p: &str| serde_json::json!({"type": "not", "path": p});
    let o = |a: &str, b: &str| serde_json::json!({"type": "or", "paths": [a, b]});
    let eq = |p: &str, v: &str| serde_json::json!({"type": "eq", "path": p, "value": v});

    let expected = serde_json::json!([
        {
            "source_expr": "architecture",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "commonAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "commonAnnotations",
            "path": ["annotations"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled"), t("commonAnnotations")],
            "resource": np
        },
        {
            "source_expr": "commonLabels",
            "path": ["from[*]", "podSelector", "matchLabels"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled"), n("networkPolicy.allowExternal")],
            "resource": np
        },
        {
            "source_expr": "commonLabels",
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "commonLabels",
            "path": ["spec", "podSelector", "matchLabels"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "commonLabels",
            "path": ["to[*]", "podSelector", "matchLabels"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled"), eq("architecture", "replication")],
            "resource": np
        },
        {
            "source_expr": "master.containerPorts.redis",
            "path": ["ingress[*]", "ports[*]", "port"],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "master.containerPorts.redis",
            "path": ["ports[*]", "port"],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), eq("architecture", "replication")],
            "resource": np
        },
        {
            "source_expr": "metrics.containerPorts.http",
            "path": ["ports[*]", "port"],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), t("metrics.enabled")],
            "resource": np
        },
        {
            "source_expr": "metrics.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.allowExternal",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.allowExternalEgress",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "networkPolicy.extraEgress",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.extraEgress",
            "path": [],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled"), t("networkPolicy.extraEgress")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.extraIngress",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.extraIngress",
            "path": [],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled"), t("networkPolicy.extraIngress")],
            "resource": np
        },
        // networkPolicy.ingressNSMatchLabels: from the `or` condition (emitted for Or guard)
        {
            "source_expr": "networkPolicy.ingressNSMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), n("networkPolicy.allowExternal")],
            "resource": np
        },
        // from nested `if .Values.networkPolicy.ingressNSMatchLabels` inside the or block
        {
            "source_expr": "networkPolicy.ingressNSMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), n("networkPolicy.allowExternal"),
                o("networkPolicy.ingressNSMatchLabels", "networkPolicy.ingressNSPodMatchLabels")
            ],
            "resource": np
        },
        // from `range` inside the if block (deduped Or guard, new Truthy guard for range target)
        {
            "source_expr": "networkPolicy.ingressNSMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), n("networkPolicy.allowExternal"),
                o("networkPolicy.ingressNSMatchLabels", "networkPolicy.ingressNSPodMatchLabels"),
                t("networkPolicy.ingressNSMatchLabels")
            ],
            "resource": np
        },
        // networkPolicy.ingressNSPodMatchLabels
        {
            "source_expr": "networkPolicy.ingressNSPodMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), n("networkPolicy.allowExternal")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.ingressNSPodMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), n("networkPolicy.allowExternal"),
                o("networkPolicy.ingressNSMatchLabels", "networkPolicy.ingressNSPodMatchLabels")
            ],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.ingressNSPodMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), n("networkPolicy.allowExternal"),
                o("networkPolicy.ingressNSMatchLabels", "networkPolicy.ingressNSPodMatchLabels"),
                t("networkPolicy.ingressNSPodMatchLabels")
            ],
            "resource": np
        },
        // metrics namespace selector guards
        {
            "source_expr": "networkPolicy.metrics.allowExternal",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), t("metrics.enabled")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.metrics.ingressNSMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), t("metrics.enabled"), n("networkPolicy.metrics.allowExternal")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.metrics.ingressNSMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), t("metrics.enabled"), n("networkPolicy.metrics.allowExternal"),
                o("networkPolicy.metrics.ingressNSMatchLabels", "networkPolicy.metrics.ingressNSPodMatchLabels")
            ],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.metrics.ingressNSMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), t("metrics.enabled"), n("networkPolicy.metrics.allowExternal"),
                o("networkPolicy.metrics.ingressNSMatchLabels", "networkPolicy.metrics.ingressNSPodMatchLabels"),
                t("networkPolicy.metrics.ingressNSMatchLabels")
            ],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.metrics.ingressNSPodMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), t("metrics.enabled"), n("networkPolicy.metrics.allowExternal")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.metrics.ingressNSPodMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), t("metrics.enabled"), n("networkPolicy.metrics.allowExternal"),
                o("networkPolicy.metrics.ingressNSMatchLabels", "networkPolicy.metrics.ingressNSPodMatchLabels")
            ],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.metrics.ingressNSPodMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), t("metrics.enabled"), n("networkPolicy.metrics.allowExternal"),
                o("networkPolicy.metrics.ingressNSMatchLabels", "networkPolicy.metrics.ingressNSPodMatchLabels"),
                t("networkPolicy.metrics.ingressNSPodMatchLabels")
            ],
            "resource": np
        },
        {
            "source_expr": "sentinel.containerPorts.sentinel",
            "path": ["port"],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), t("sentinel.enabled")],
            "resource": np
        },
        {
            "source_expr": "sentinel.containerPorts.sentinel",
            "path": ["port"],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), eq("architecture", "replication"), t("sentinel.enabled")],
            "resource": np
        },
        {
            "source_expr": "sentinel.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "sentinel.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), eq("architecture", "replication")],
            "resource": np
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
