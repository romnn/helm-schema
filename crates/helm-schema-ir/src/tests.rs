use crate::{
    DefaultIrGenerator, DefaultResourceDetector, Guard, IrGenerator, ResourceDetector, ResourceRef,
    ValueKind, YamlPath,
};
use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};

fn prometheusrule_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/prometheusrule.yaml"
    );
    std::fs::read_to_string(path).expect("read prometheusrule.yaml")
}

fn helpers_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/_helpers.tpl"
    );
    std::fs::read_to_string(path).expect("read _helpers.tpl")
}

fn common_helpers_srcs() -> Vec<String> {
    let base = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/charts/common/templates"
    );
    let mut srcs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            if entry.path().extension().map_or(false, |e| e == "tpl") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    srcs.push(content);
                }
            }
        }
    }
    srcs
}

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(parser, &helpers_src()).expect("helpers");
    for src in common_helpers_srcs() {
        let _ = idx.add_source(parser, &src);
    }
    idx
}

/// DefaultResourceDetector finds the PrometheusRule resource type.
#[test]
fn resource_detection_prometheusrule() {
    let src = prometheusrule_src();
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

/// Both parsers should produce equivalent IR for the prometheusrule template.
#[test]
fn both_parsers_produce_same_ir_prometheusrule() {
    let src = prometheusrule_src();

    let rust_ast = FusedRustParser.parse(&src).expect("fused rust");
    let rust_idx = build_define_index(&FusedRustParser);
    let rust_ir = DefaultIrGenerator.generate(&rust_ast, &rust_idx);

    let ts_ast = TreeSitterParser.parse(&src).expect("tree-sitter");
    let ts_idx = build_define_index(&TreeSitterParser);
    let ts_ir = DefaultIrGenerator.generate(&ts_ast, &ts_idx);

    // Both should find .Values.metrics.enabled
    assert!(
        rust_ir.iter().any(|u| u.source_expr == "metrics.enabled"),
        "rust IR should contain metrics.enabled"
    );
    assert!(
        ts_ir.iter().any(|u| u.source_expr == "metrics.enabled"),
        "ts IR should contain metrics.enabled"
    );

    // Both should find .Values.metrics.prometheusRule.enabled
    assert!(
        rust_ir
            .iter()
            .any(|u| u.source_expr == "metrics.prometheusRule.enabled"),
        "rust IR should contain metrics.prometheusRule.enabled"
    );
    assert!(
        ts_ir
            .iter()
            .any(|u| u.source_expr == "metrics.prometheusRule.enabled"),
        "ts IR should contain metrics.prometheusRule.enabled"
    );

    // Both should detect the PrometheusRule resource
    let rust_has_resource = rust_ir.iter().any(|u| {
        u.resource
            .as_ref()
            .map_or(false, |r| r.kind == "PrometheusRule")
    });
    let ts_has_resource = ts_ir.iter().any(|u| {
        u.resource
            .as_ref()
            .map_or(false, |r| r.kind == "PrometheusRule")
    });
    assert!(rust_has_resource, "rust IR should detect PrometheusRule");
    assert!(ts_has_resource, "ts IR should detect PrometheusRule");
}

/// IR from the fused-Rust parser has the full expected content for prometheusrule.
#[test]
fn fused_rust_ir_prometheusrule_full() {
    let src = prometheusrule_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = DefaultIrGenerator.generate(&ast, &idx);

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
            "path": ["annotations"],
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
            "path": [],
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

// ---------------------------------------------------------------------------
// networkpolicy.yaml tests
// ---------------------------------------------------------------------------

fn networkpolicy_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
    );
    std::fs::read_to_string(path).expect("read networkpolicy.yaml")
}

/// Resource detection for networkpolicy (kind is scalar, apiVersion is template).
#[test]
fn resource_detection_networkpolicy() {
    let src = networkpolicy_src();
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
///
/// Currently ignored due to two fundamental fused-rust parser limitations:
///
/// 1. **YAML path loss**: The fused-rust parser parses YAML in separate fragments
///    per Helm control flow block, losing nesting context. For example, `from:`
///    inside `{{- if not .Values.networkPolicy.allowExternal }}` produces
///    `["podSelector", "matchLabels"]` instead of the correct
///    `["from[*]", "podSelector", "matchLabels"]` (tree-sitter is correct).
///
/// 2. **Extra range entries**: The fused-rust parser produces additional `ValueUse`
///    entries from `range` blocks inside `if` chains that tree-sitter doesn't emit,
///    due to different AST nesting of `if`→`range` chains.
///
/// Both issues require deep changes to the yaml-rust fork's line-by-line approach.
/// The tree-sitter parser is the reference implementation for correctness.
#[test]
#[ignore = "parser parity: fused-rust loses YAML nesting context across Helm control flow boundaries"]
fn both_parsers_produce_same_ir_networkpolicy() {
    let src = networkpolicy_src();

    let rust_ast = FusedRustParser.parse(&src).expect("fused rust");
    let rust_idx = build_define_index(&FusedRustParser);
    let rust_ir = DefaultIrGenerator.generate(&rust_ast, &rust_idx);

    let ts_ast = TreeSitterParser.parse(&src).expect("tree-sitter");
    let ts_idx = build_define_index(&TreeSitterParser);
    let ts_ir = DefaultIrGenerator.generate(&ts_ast, &ts_idx);

    let rust_json: serde_json::Value = serde_json::to_value(&rust_ir).expect("serialize");
    let ts_json: serde_json::Value = serde_json::to_value(&ts_ir).expect("serialize");
    similar_asserts::assert_eq!(rust_json, ts_json);
}

/// Full hardcoded expected IR for networkpolicy using fused-Rust parser.
///
/// Now uses structured Guard types: Truthy, Not, Or, Eq.
/// Guards are deduplicated (no duplicate from if→range chains).
/// `not` conditions produce Guard::Not, `or` produces Guard::Or, `eq` produces Guard::Eq.
#[test]
fn fused_rust_ir_networkpolicy_full() {
    let src = networkpolicy_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
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
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": [t("networkPolicy.enabled")],
            "resource": np
        },
        {
            "source_expr": "commonLabels",
            "path": ["podSelector", "matchLabels"],
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

/// Simple template IR generation test.
#[test]
fn simple_template_ir() {
    let src = r#"{{- if .Values.enabled }}
foo: {{ .Values.name }}
{{- end }}
"#;
    let ast = FusedRustParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    let ir = DefaultIrGenerator.generate(&ast, &idx);

    assert!(
        ir.iter()
            .any(|u| u.source_expr == "enabled" && u.guards.is_empty())
    );
    assert!(ir.iter().any(|u| u.source_expr == "name"
        && u.path == YamlPath(vec!["foo".to_string()])
        && u.kind == ValueKind::Scalar
        && u.guards
            == vec![Guard::Truthy {
                path: "enabled".to_string()
            }]));
}
