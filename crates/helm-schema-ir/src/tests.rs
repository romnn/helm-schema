use crate::{
    DefaultIrGenerator, DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef,
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

    let expected = serde_json::json!([
        {
            "source_expr": "commonAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": {"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"}
        },
        {
            "source_expr": "commonAnnotations",
            "path": ["annotations"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "commonAnnotations"],
            "resource": {"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"}
        },
        {
            "source_expr": "commonLabels",
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": {"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"}
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
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": {"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"}
        },
        {
            "source_expr": "metrics.prometheusRule.additionalLabels",
            "path": [],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "metrics.prometheusRule.additionalLabels"],
            "resource": {"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"}
        },
        {
            "source_expr": "metrics.prometheusRule.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": ["metrics.enabled"],
            "resource": null
        },
        {
            "source_expr": "metrics.prometheusRule.namespace",
            "path": ["metadata", "namespace"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": {"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"}
        },
        {
            "source_expr": "metrics.prometheusRule.rules",
            "path": ["spec", "groups[*]", "rules"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": {"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"}
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
        && u.guards == vec!["enabled".to_string()]));
}
