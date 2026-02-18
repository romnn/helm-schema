#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

const TEMPLATE_PATH: &str = "charts/surveyor/templates/configmap.yaml";

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
            api_version: "v1".to_string(),
            kind: "ConfigMap".to_string(),
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

    let cm = serde_json::json!({"api_version": "v1", "kind": "ConfigMap"});
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    let expected = serde_json::json!([
        {
            "source_expr": "config.jetstream.accounts",
            "path": [],
            "kind": "Scalar",
            "guards": [t("config.jetstream.enabled")],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*",
            "path": [],
            "kind": "Scalar",
            "guards": [t("config.jetstream.enabled"), t("config.jetstream.accounts")],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.name",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("config.jetstream.enabled"),
                t("config.jetstream.accounts"),
                t("config.jetstream.accounts.*.tls"),
                t("config.jetstream.accounts.*.tls.ca")
            ],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.name",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("config.jetstream.enabled"),
                t("config.jetstream.accounts"),
                t("config.jetstream.accounts.*.tls"),
                t("config.jetstream.accounts.*.tls.cert")
            ],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.name",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("config.jetstream.enabled"),
                t("config.jetstream.accounts"),
                t("config.jetstream.accounts.*.tls"),
                t("config.jetstream.accounts.*.tls.key")
            ],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.tls",
            "path": [],
            "kind": "Scalar",
            "guards": [t("config.jetstream.enabled"), t("config.jetstream.accounts")],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.tls.ca",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("config.jetstream.enabled"),
                t("config.jetstream.accounts"),
                t("config.jetstream.accounts.*.tls")
            ],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.tls.ca",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("config.jetstream.enabled"),
                t("config.jetstream.accounts"),
                t("config.jetstream.accounts.*.tls"),
                t("config.jetstream.accounts.*.tls.ca")
            ],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.tls.cert",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("config.jetstream.enabled"),
                t("config.jetstream.accounts"),
                t("config.jetstream.accounts.*.tls")
            ],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.tls.cert",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("config.jetstream.enabled"),
                t("config.jetstream.accounts"),
                t("config.jetstream.accounts.*.tls"),
                t("config.jetstream.accounts.*.tls.cert")
            ],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.tls.key",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("config.jetstream.enabled"),
                t("config.jetstream.accounts"),
                t("config.jetstream.accounts.*.tls")
            ],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.accounts.*.tls.key",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("config.jetstream.enabled"),
                t("config.jetstream.accounts"),
                t("config.jetstream.accounts.*.tls"),
                t("config.jetstream.accounts.*.tls.key")
            ],
            "resource": cm
        },
        {
            "source_expr": "config.jetstream.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
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
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
