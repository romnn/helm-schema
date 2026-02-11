#![recursion_limit = "512"]

mod common;

use helm_schema_ast::{FusedRustParser, HelmParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::UpstreamK8sSchemaProvider;

#[test]
fn schema_fused_rust() {
    let src = common::cert_manager_deployment_src();
    let values_yaml = common::cert_manager_values_yaml_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = common::build_cert_manager_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/kubernetes-json-schema"
        ))
        .with_allow_download(false);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "acmesolver": {
                "additionalProperties": false,
                "properties": {
                    "image": {
                        "additionalProperties": false,
                        "properties": {
                            "digest": {},
                            "pullPolicy": {"type": "string"},
                            "registry": {},
                            "repository": {"type": "string"},
                            "tag": {}
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            },
            "affinity": {
                "properties": {
                    "podAntiAffinity": {"type": "object"}
                },
                "type": "object"
            },
            "automountServiceAccountToken": {},
            "clusterResourceNamespace": {"type": "string"},
            "config": {
                "additionalProperties": {},
                "type": "object"
            },
            "containerSecurityContext": {
                "additionalProperties": false,
                "properties": {
                    "allowPrivilegeEscalation": {"type": "boolean"},
                    "capabilities": {
                        "additionalProperties": false,
                        "properties": {
                            "drop": {
                                "items": {"type": "string"},
                                "type": "array"
                            }
                        },
                        "type": "object"
                    },
                    "readOnlyRootFilesystem": {"type": "boolean"}
                },
                "type": "object"
            },
            "deploymentAnnotations": {
                "additionalProperties": {},
                "type": "object"
            },
            "disableAutoApproval": {"type": "boolean"},
            "dns01RecursiveNameservers": {"type": "string"},
            "dns01RecursiveNameserversOnly": {"type": "boolean"},
            "enableCertificateOwnerRef": {"type": "boolean"},
            "enableServiceLinks": {"type": "boolean"},
            "extraArgs": {
                "items": {},
                "type": "array"
            },
            "extraEnv": {
                "items": {},
                "type": "array"
            },
            "featureGates": {"type": "string"},
            "global": {
                "additionalProperties": false,
                "properties": {
                    "commonLabels": {
                        "additionalProperties": {},
                        "type": "object"
                    },
                    "hostUsers": {},
                    "imagePullSecrets": {
                        "items": {},
                        "type": "array"
                    },
                    "leaderElection": {
                        "additionalProperties": false,
                        "properties": {
                            "leaseDuration": {},
                            "namespace": {"type": "string"},
                            "renewDeadline": {},
                            "retryPeriod": {}
                        },
                        "type": "object"
                    },
                    "logLevel": {"type": "integer"},
                    "nodeSelector": {
                        "additionalProperties": {},
                        "type": "object"
                    },
                    "podSecurityPolicy": {
                        "additionalProperties": false,
                        "properties": {
                            "enabled": {"type": "boolean"},
                            "useAppArmor": {"type": "boolean"}
                        },
                        "type": "object"
                    },
                    "priorityClassName": {"type": "string"},
                    "rbac": {
                        "additionalProperties": false,
                        "properties": {
                            "aggregateClusterRoles": {"type": "boolean"},
                            "create": {"type": "boolean"}
                        },
                        "type": "object"
                    },
                    "revisionHistoryLimit": {"type": "boolean"}
                },
                "type": "object"
            },
            "hostAliases": {
                "items": {},
                "type": "array"
            },
            "http_proxy": {},
            "https_proxy": {},
            "image": {
                "additionalProperties": false,
                "properties": {
                    "pullPolicy": {"type": "string"}
                },
                "type": "object"
            },
            "ingressShim": {
                "additionalProperties": false,
                "properties": {
                    "defaultIssuerGroup": {},
                    "defaultIssuerKind": {},
                    "defaultIssuerName": {}
                },
                "type": "object"
            },
            "livenessProbe": {
                "additionalProperties": false,
                "properties": {
                    "enabled": {"type": "boolean"},
                    "failureThreshold": {"type": "integer"},
                    "initialDelaySeconds": {"type": "integer"},
                    "periodSeconds": {"type": "integer"},
                    "successThreshold": {"type": "integer"},
                    "timeoutSeconds": {"type": "integer"}
                },
                "type": "object"
            },
            "maxConcurrentChallenges": {"type": "integer"},
            "no_proxy": {},
            "nodeSelector": {
                "additionalProperties": false,
                "properties": {
                    "kubernetes.io/os": {"type": "string"}
                },
                "type": "object"
            },
            "podAnnotations": {
                "additionalProperties": {},
                "type": "object"
            },
            "podDnsConfig": {
                "additionalProperties": {},
                "type": "object"
            },
            "podDnsPolicy": {},
            "podLabels": {
                "additionalProperties": {},
                "type": "object"
            },
            "prometheus": {
                "additionalProperties": false,
                "properties": {
                    "enabled": {"type": "boolean"},
                    "podmonitor": {
                        "additionalProperties": false,
                        "properties": {
                            "enabled": {"type": "boolean"}
                        },
                        "type": "object"
                    },
                    "servicemonitor": {
                        "additionalProperties": false,
                        "properties": {
                            "enabled": {"type": "boolean"}
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            },
            "replicaCount": {"type": "integer"},
            "resources": {
                "additionalProperties": {},
                "type": "object"
            },
            "securityContext": {
                "properties": {
                    "runAsNonRoot": {"type": "boolean"}
                },
                "type": "object"
            },
            "serviceAccount": {
                "additionalProperties": false,
                "properties": {
                    "create": {"type": "boolean"}
                },
                "type": "object"
            },
            "strategy": {
                "additionalProperties": {},
                "type": "object"
            },
            "tolerations": {
                "items": {},
                "type": "array"
            },
            "topologySpreadConstraints": {
                "items": {},
                "type": "array"
            },
            "volumeMounts": {
                "items": {},
                "type": "array"
            },
            "volumes": {
                "items": {
                    "properties": {
                        "emptyDir": {"type": "object"},
                        "name": {"type": "string"}
                    },
                    "type": "object"
                },
                "type": "array"
            }
        },
        "type": "object"
    });

    similar_asserts::assert_eq!(actual, expected);
}
