use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/bitnami-redis/templates/_helpers.tpl"),
    )
    .expect("helpers");
    for src in test_util::read_testdata_dir("charts/bitnami-redis/charts/common/templates", "tpl") {
        let _ = idx.add_source(parser, &src);
    }
    idx
}

/// Resource detection for networkpolicy (kind is scalar, apiVersion is template).
#[test]
fn resource_detection() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/networkpolicy.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    // apiVersion is a template call, not a scalar, so it won't be detected.
    // kind: NetworkPolicy IS a scalar.
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: String::new(),
            kind: "NetworkPolicy".to_string(),
            api_version_candidates: Vec::new(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/networkpolicy.yaml");
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
            "path": ["metadata", "annotations"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled"), t("commonAnnotations")],
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
            "path": ["spec", "egress[*]", "to[*]", "podSelector", "matchLabels"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled"), eq("architecture", "replication")],
            "resource": np
        },
        {
            "source_expr": "commonLabels",
            "path": ["spec", "ingress[*]", "from[*]", "podSelector", "matchLabels"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled"), n("networkPolicy.allowExternal")],
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
            "source_expr": "master.containerPorts.redis",
            "path": ["spec", "egress[*]", "ports[*]", "port"],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), eq("architecture", "replication")],
            "resource": np
        },
        {
            "source_expr": "master.containerPorts.redis",
            "path": ["spec", "ingress[*]", "ports[*]", "port"],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "metrics.containerPorts.http",
            "path": ["spec", "ingress[*]", "ports[*]", "port"],
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
            "path": ["spec", "egress"],
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
            "path": ["spec", "ingress"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled"), t("networkPolicy.extraIngress")],
            "resource": np
        },
        {
            "source_expr": "networkPolicy.ingressNSMatchLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), n("networkPolicy.allowExternal")],
            "resource": np
        },
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
        {
            "source_expr": "networkPolicy.ingressNSMatchLabels",
            "path": ["spec", "ingress[*]", "from[*]", "namespaceSelector", "matchLabels"],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), n("networkPolicy.allowExternal"),
                o("networkPolicy.ingressNSMatchLabels", "networkPolicy.ingressNSPodMatchLabels"),
                t("networkPolicy.ingressNSMatchLabels")
            ],
            "resource": np
        },
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
            "path": ["spec", "ingress[*]", "from[*]", "podSelector", "matchLabels"],
            "kind": "Scalar",
            "guards": [
                t("networkPolicy.enabled"), n("networkPolicy.allowExternal"),
                o("networkPolicy.ingressNSMatchLabels", "networkPolicy.ingressNSPodMatchLabels"),
                t("networkPolicy.ingressNSPodMatchLabels")
            ],
            "resource": np
        },
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
            "path": ["spec", "ingress[*]", "from[*]", "namespaceSelector", "matchLabels"],
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
            "path": ["spec", "ingress[*]", "from[*]", "podSelector", "matchLabels"],
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
            "path": ["spec", "egress[*]", "ports[*]", "port"],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), eq("architecture", "replication"), t("sentinel.enabled")],
            "resource": np
        },
        {
            "source_expr": "sentinel.containerPorts.sentinel",
            "path": ["spec", "ingress[*]", "ports[*]", "port"],
            "kind": "Scalar",
            "guards": [t("networkPolicy.enabled"), t("sentinel.enabled")],
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
