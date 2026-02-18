#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/zalando-postgres-operator/templates/_helpers.tpl"),
    )
    .expect("helpers");
    idx
}

#[test]
fn resource_detection() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/deployment.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            api_version_candidates: Vec::new(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/deployment.yaml");
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

    let deploy = serde_json::json!({"api_version": "apps/v1", "kind": "Deployment"});
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    let expected = serde_json::json!([
        {
            "guards": [],
            "kind": "Fragment",
            "path": ["spec", "template", "spec", "affinity"],
            "resource": deploy,
            "source_expr": "affinity"
        },
        {
            "guards": [t("readinessProbe")],
            "kind": "Scalar",
            "path": [
                "spec",
                "template",
                "spec",
                "containers[*]",
                "readinessProbe",
                "httpGet",
                "port"
            ],
            "resource": deploy,
            "source_expr": "configLoggingRestApi.api_port"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": deploy,
            "source_expr": "configTarget"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": deploy,
            "source_expr": "controllerID.create"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": null,
            "source_expr": "controllerID.name"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": deploy,
            "source_expr": "enableJsonLogging"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": deploy,
            "source_expr": "extraEnvs"
        },
        {
            "guards": [t("extraEnvs")],
            "kind": "Fragment",
            "path": ["spec", "template", "spec", "containers"],
            "resource": deploy,
            "source_expr": "extraEnvs"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": null,
            "source_expr": "fullnameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [
                "spec",
                "template",
                "spec",
                "containers[*]",
                "imagePullPolicy"
            ],
            "resource": deploy,
            "source_expr": "image.pullPolicy"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": ["spec", "template", "spec", "containers[*]", "image"],
            "resource": deploy,
            "source_expr": "image.registry"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": ["spec", "template", "spec", "containers[*]", "image"],
            "resource": deploy,
            "source_expr": "image.repository"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": ["spec", "template", "spec", "containers[*]", "image"],
            "resource": deploy,
            "source_expr": "image.tag"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": deploy,
            "source_expr": "imagePullSecrets"
        },
        {
            "guards": [t("imagePullSecrets")],
            "kind": "Fragment",
            "path": ["spec", "template", "spec", "imagePullSecrets"],
            "resource": deploy,
            "source_expr": "imagePullSecrets"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": [],
            "kind": "Fragment",
            "path": ["spec", "template", "spec", "nodeSelector"],
            "resource": deploy,
            "source_expr": "nodeSelector"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": deploy,
            "source_expr": "podAnnotations"
        },
        {
            "guards": [t("podAnnotations")],
            "kind": "Fragment",
            "path": ["spec", "template", "metadata"],
            "resource": deploy,
            "source_expr": "podAnnotations"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": deploy,
            "source_expr": "podLabels"
        },
        {
            "guards": [t("podLabels")],
            "kind": "Fragment",
            "path": ["spec", "template", "metadata", "labels"],
            "resource": deploy,
            "source_expr": "podLabels"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": deploy,
            "source_expr": "priorityClassName"
        },
        {
            "guards": [t("priorityClassName")],
            "kind": "Scalar",
            "path": ["spec", "template", "spec", "priorityClassName"],
            "resource": deploy,
            "source_expr": "priorityClassName"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": deploy,
            "source_expr": "readinessProbe"
        },
        {
            "guards": [t("readinessProbe")],
            "kind": "Scalar",
            "path": [
                "spec",
                "template",
                "spec",
                "containers[*]",
                "readinessProbe",
                "initialDelaySeconds"
            ],
            "resource": deploy,
            "source_expr": "readinessProbe.initialDelaySeconds"
        },
        {
            "guards": [t("readinessProbe")],
            "kind": "Scalar",
            "path": [
                "spec",
                "template",
                "spec",
                "containers[*]",
                "readinessProbe",
                "periodSeconds"
            ],
            "resource": deploy,
            "source_expr": "readinessProbe.periodSeconds"
        },
        {
            "guards": [],
            "kind": "Fragment",
            "path": ["spec", "template", "spec", "containers[*]", "resources"],
            "resource": deploy,
            "source_expr": "resources"
        },
        {
            "guards": [],
            "kind": "Fragment",
            "path": ["spec", "template", "spec", "containers[*]", "securityContext"],
            "resource": deploy,
            "source_expr": "securityContext"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": null,
            "source_expr": "serviceAccount.name"
        },
        {
            "guards": [],
            "kind": "Fragment",
            "path": ["spec", "template", "spec", "tolerations"],
            "resource": deploy,
            "source_expr": "tolerations"
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
