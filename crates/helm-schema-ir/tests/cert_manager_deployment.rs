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
    let src = test_util::read_testdata("charts/cert-manager/templates/deployment.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src = test_util::read_testdata("charts/cert-manager/templates/deployment.yaml");
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

    let dep = serde_json::json!({"api_version": "apps/v1", "kind": "Deployment"});
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let n = |p: &str| serde_json::json!({"type": "not", "path": p});
    let o = |a: &str, b: &str| serde_json::json!({"type": "or", "paths": [a, b]});

    let expected = serde_json::json!([
        {
            "source_expr": "acmesolver.image",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "acmesolver.image.digest",
            "path": [],
            "kind": "Scalar",
            "guards": [t("acmesolver.image")],
            "resource": dep
        },
        {
            "source_expr": "acmesolver.image.registry",
            "path": [],
            "kind": "Scalar",
            "guards": [t("acmesolver.image")],
            "resource": dep
        },
        {
            "source_expr": "acmesolver.image.tag",
            "path": ["spec", "template", "spec", "containers[*]", "args[*]"],
            "kind": "Scalar",
            "guards": [t("acmesolver.image")],
            "resource": dep
        },
        {
            "source_expr": "affinity",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "affinity",
            "path": ["spec", "template", "spec", "affinity"],
            "kind": "Fragment",
            "guards": [t("affinity")],
            "resource": dep
        },
        {
            "source_expr": "automountServiceAccountToken",
            "path": ["spec", "template", "spec", "automountServiceAccountToken"],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "clusterResourceNamespace",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "clusterResourceNamespace",
            "path": ["spec", "template", "spec", "containers[*]", "args[*]"],
            "kind": "Scalar",
            "guards": [t("clusterResourceNamespace")],
            "resource": dep
        },
        {
            "source_expr": "config",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "config",
            "path": [],
            "kind": "Scalar",
            "guards": [o("config", "volumeMounts")],
            "resource": dep
        },
        {
            "source_expr": "config",
            "path": [],
            "kind": "Scalar",
            "guards": [o("config", "volumes")],
            "resource": dep
        },
        {
            "source_expr": "containerSecurityContext",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "containerSecurityContext",
            "path": ["spec", "template", "spec", "containers[*]", "args"],
            "kind": "Fragment",
            "guards": [t("containerSecurityContext")],
            "resource": dep
        },
        {
            "source_expr": "deploymentAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "deploymentAnnotations",
            "path": ["metadata", "annotations"],
            "kind": "Fragment",
            "guards": [t("deploymentAnnotations")],
            "resource": dep
        },
        {
            "source_expr": "disableAutoApproval",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "dns01RecursiveNameservers",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "dns01RecursiveNameserversOnly",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "enableCertificateOwnerRef",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "enableServiceLinks",
            "path": ["spec", "template", "spec", "enableServiceLinks"],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "extraArgs",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "extraArgs",
            "path": ["spec", "template", "spec", "containers[*]", "args"],
            "kind": "Fragment",
            "guards": [t("extraArgs")],
            "resource": dep
        },
        {
            "source_expr": "extraEnv",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "extraEnv",
            "path": ["spec", "template", "spec", "containers[*]", "args"],
            "kind": "Fragment",
            "guards": [t("extraEnv")],
            "resource": dep
        },
        {
            "source_expr": "featureGates",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "featureGates",
            "path": ["spec", "template", "spec", "containers[*]", "args[*]"],
            "kind": "Scalar",
            "guards": [t("featureGates")],
            "resource": dep
        },
        {
            "source_expr": "fullnameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "global",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "global.hostUsers",
            "path": ["spec", "template", "spec", "hostUsers"],
            "kind": "Scalar",
            "guards": [t("global")],
            "resource": dep
        },
        {
            "source_expr": "global.imagePullSecrets",
            "path": [],
            "kind": "Scalar",
            "guards": [n("serviceAccount.create")],
            "resource": dep
        },
        {
            "source_expr": "global.imagePullSecrets",
            "path": ["spec", "template", "spec", "imagePullSecrets"],
            "kind": "Fragment",
            "guards": [n("serviceAccount.create"), t("global.imagePullSecrets")],
            "resource": dep
        },
        {
            "source_expr": "global.leaderElection",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "global.leaderElection.leaseDuration",
            "path": [],
            "kind": "Scalar",
            "guards": [t("global.leaderElection")],
            "resource": dep
        },
        {
            "source_expr": "global.leaderElection.renewDeadline",
            "path": [],
            "kind": "Scalar",
            "guards": [t("global.leaderElection")],
            "resource": dep
        },
        {
            "source_expr": "global.leaderElection.retryPeriod",
            "path": [],
            "kind": "Scalar",
            "guards": [t("global.leaderElection")],
            "resource": dep
        },
        {
            "source_expr": "global.logLevel",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "global.logLevel",
            "path": ["spec", "template", "spec", "containers[*]", "args[*]"],
            "kind": "Scalar",
            "guards": [n("global.logLevel")],
            "resource": dep
        },
        {
            "source_expr": "global.nodeSelector",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "global.priorityClassName",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "global.priorityClassName",
            "path": ["spec", "template", "spec", "priorityClassName"],
            "kind": "Scalar",
            "guards": [t("global.priorityClassName")],
            "resource": dep
        },
        {
            "source_expr": "global.revisionHistoryLimit",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "global.revisionHistoryLimit",
            "path": ["spec", "revisionHistoryLimit"],
            "kind": "Scalar",
            "guards": [n("global.revisionHistoryLimit")],
            "resource": dep
        },
        {
            "source_expr": "hostAliases",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "hostAliases",
            "path": ["spec", "template", "spec", "hostAliases"],
            "kind": "Fragment",
            "guards": [t("hostAliases")],
            "resource": dep
        },
        {
            "source_expr": "http_proxy",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "https_proxy",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "image",
            "path": ["spec", "template", "spec", "containers[*]", "image"],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "image.pullPolicy",
            "path": ["spec", "template", "spec", "containers[*]", "imagePullPolicy"],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "ingressShim",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "ingressShim.defaultIssuerGroup",
            "path": [],
            "kind": "Scalar",
            "guards": [t("ingressShim")],
            "resource": dep
        },
        {
            "source_expr": "ingressShim.defaultIssuerKind",
            "path": [],
            "kind": "Scalar",
            "guards": [t("ingressShim")],
            "resource": dep
        },
        {
            "source_expr": "ingressShim.defaultIssuerName",
            "path": [],
            "kind": "Scalar",
            "guards": [t("ingressShim")],
            "resource": dep
        },
        {
            "source_expr": "livenessProbe",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "livenessProbe.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("livenessProbe")],
            "resource": dep
        },
        {
            "source_expr": "maxConcurrentChallenges",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "maxConcurrentChallenges",
            "path": ["spec", "template", "spec", "containers[*]", "args[*]"],
            "kind": "Scalar",
            "guards": [t("maxConcurrentChallenges")],
            "resource": dep
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
            "source_expr": "no_proxy",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "nodeSelector",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "podAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "podAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": [
                t("prometheus.enabled"),
                t("prometheus.podmonitor.enabled"),
                t("prometheus.servicemonitor.enabled")
            ],
            "resource": dep
        },
        {
            "source_expr": "podAnnotations",
            "path": ["spec", "template", "metadata", "annotations"],
            "kind": "Fragment",
            "guards": [t("podAnnotations")],
            "resource": dep
        },
        {
            "source_expr": "podDnsConfig",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "podDnsConfig",
            "path": ["spec", "template", "spec", "dnsConfig"],
            "kind": "Fragment",
            "guards": [t("podDnsConfig")],
            "resource": dep
        },
        {
            "source_expr": "podDnsPolicy",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "podLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "podLabels",
            "path": ["spec", "template", "metadata", "labels"],
            "kind": "Fragment",
            "guards": [t("podLabels")],
            "resource": dep
        },
        {
            "source_expr": "prometheus.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "prometheus.podmonitor.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("prometheus.enabled")],
            "resource": dep
        },
        {
            "source_expr": "prometheus.servicemonitor.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("prometheus.enabled"), t("prometheus.podmonitor.enabled")],
            "resource": dep
        },
        {
            "source_expr": "replicaCount",
            "path": ["spec", "replicas"],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "resources",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "resources",
            "path": ["spec", "template", "spec", "containers[*]", "args"],
            "kind": "Fragment",
            "guards": [t("resources")],
            "resource": dep
        },
        {
            "source_expr": "securityContext",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "securityContext",
            "path": ["spec", "template", "spec", "securityContext"],
            "kind": "Fragment",
            "guards": [t("securityContext")],
            "resource": dep
        },
        {
            "source_expr": "serviceAccount.create",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "serviceAccount.create",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "serviceAccount.name",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "strategy",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "strategy",
            "path": ["spec", "strategy"],
            "kind": "Fragment",
            "guards": [t("strategy")],
            "resource": dep
        },
        {
            "source_expr": "tolerations",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "tolerations",
            "path": ["spec", "template", "spec", "tolerations"],
            "kind": "Fragment",
            "guards": [t("tolerations")],
            "resource": dep
        },
        {
            "source_expr": "topologySpreadConstraints",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "topologySpreadConstraints",
            "path": ["spec", "template", "spec", "topologySpreadConstraints"],
            "kind": "Fragment",
            "guards": [t("topologySpreadConstraints")],
            "resource": dep
        },
        {
            "source_expr": "volumeMounts",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "volumeMounts",
            "path": [],
            "kind": "Scalar",
            "guards": [o("config", "volumeMounts")],
            "resource": dep
        },
        {
            "source_expr": "volumeMounts",
            "path": ["spec", "template", "spec", "containers[*]", "args"],
            "kind": "Fragment",
            "guards": [o("config", "volumeMounts"), t("volumeMounts")],
            "resource": dep
        },
        {
            "source_expr": "volumes",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": dep
        },
        {
            "source_expr": "volumes",
            "path": [],
            "kind": "Scalar",
            "guards": [o("config", "volumes")],
            "resource": dep
        },
        {
            "source_expr": "volumes",
            "path": ["spec", "template", "spec", "volumes"],
            "kind": "Fragment",
            "guards": [o("config", "volumes"), t("volumes")],
            "resource": dep
        }
    ]);

    similar_asserts::assert_eq!(have: actual, want: expected);
}
