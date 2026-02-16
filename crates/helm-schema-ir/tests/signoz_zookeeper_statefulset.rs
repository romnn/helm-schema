#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

const TEMPLATE_PATH: &str =
    "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml";
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
            api_version: String::new(),
            kind: "StatefulSet".to_string(),
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

    let actual = serde_json::to_value(&ir).unwrap();

    if std::env::var("IR_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let expected = serde_json::json!(
    [
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "affinity"
      },
      {
        "guards": [
          {
            "path": "affinity",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "affinity"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "affinity"
      },
      {
        "guards": [
          {
            "path": "auth.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "auth.client.clientUser"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "auth.client.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "auth.client.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "auth.client.existingSecret"
      },
      {
        "guards": [
          {
            "path": "auth.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "auth.client.serverUsers"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "auth.quorum.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "auth.quorum.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "auth.quorum.existingSecret"
      },
      {
        "guards": [
          {
            "path": "auth.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "auth.quorum.learnerUser"
      },
      {
        "guards": [
          {
            "path": "auth.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "auth.quorum.serverUsers"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "autopurge.purgeInterval"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "autopurge.snapRetainCount"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "clusterDomain"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "commonAnnotations"
      },
      {
        "guards": [
          {
            "path": "commonAnnotations",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "metadata",
          "annotations"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "commonAnnotations"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "commonLabels"
      },
      {
        "guards": [
          {
            "path": "commonLabels",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "metadata",
          "labels"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "commonLabels"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "configuration"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerPorts.client"
      },
      {
        "guards": [
          {
            "path": "service.disableBaseClientPort",
            "type": "not"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "ports[*]",
          "containerPort"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerPorts.client"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerPorts.election"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "ports[*]",
          "containerPort"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerPorts.election"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerPorts.follower"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "ports[*]",
          "containerPort"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerPorts.follower"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerPorts.tls"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "ports[*]",
          "containerPort"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerPorts.tls"
      },
      {
        "guards": [
          {
            "path": "containerSecurityContext.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "securityContext"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerSecurityContext"
      },
      {
        "guards": [
          {
            "paths": [
              "tls.client.enabled",
              "tls.quorum.enabled"
            ],
            "type": "or"
          },
          {
            "path": "containerSecurityContext.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "securityContext"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerSecurityContext"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerSecurityContext.enabled"
      },
      {
        "guards": [
          {
            "paths": [
              "tls.client.enabled",
              "tls.quorum.enabled"
            ],
            "type": "or"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerSecurityContext.enabled"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "args[*]"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerSecurityContext.runAsUser"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "args[*]"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "containerSecurityContext.runAsUser"
      },
      {
        "guards": [
          {
            "path": "diagnosticMode.enabled",
            "type": "not"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "customLivenessProbe"
      },
      {
        "guards": [
          {
            "path": "diagnosticMode.enabled",
            "type": "not"
          },
          {
            "path": "customLivenessProbe",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "livenessProbe"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "customLivenessProbe"
      },
      {
        "guards": [
          {
            "path": "diagnosticMode.enabled",
            "type": "not"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "customReadinessProbe"
      },
      {
        "guards": [
          {
            "path": "diagnosticMode.enabled",
            "type": "not"
          },
          {
            "path": "customReadinessProbe",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "readinessProbe"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "customReadinessProbe"
      },
      {
        "guards": [
          {
            "path": "diagnosticMode.enabled",
            "type": "not"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "customStartupProbe"
      },
      {
        "guards": [
          {
            "path": "diagnosticMode.enabled",
            "type": "not"
          },
          {
            "path": "customStartupProbe",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "startupProbe"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "customStartupProbe"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "dataLogDir"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "dataLogDir"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "dataLogDir"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "dataLogDir"
      },
      {
        "guards": [
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "volumeMounts[*]",
          "mountPath"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "dataLogDir"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "args[*]"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "dataLogDir"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "volumeMounts[*]",
          "mountPath"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "dataLogDir"
      },
      {
        "guards": [
          {
            "path": "diagnosticMode.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "args"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "diagnosticMode.args"
      },
      {
        "guards": [
          {
            "path": "diagnosticMode.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "command"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "diagnosticMode.command"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "diagnosticMode.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "diagnosticMode.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "existingConfigmap"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "existingConfigmap"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraEnvVars"
      },
      {
        "guards": [
          {
            "path": "extraEnvVars",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraEnvVars"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraEnvVarsCM"
      },
      {
        "guards": [
          {
            "paths": [
              "extraEnvVarsCM",
              "extraEnvVarsSecret"
            ],
            "type": "or"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraEnvVarsCM"
      },
      {
        "guards": [
          {
            "paths": [
              "extraEnvVarsCM",
              "extraEnvVarsSecret"
            ],
            "type": "or"
          },
          {
            "path": "extraEnvVarsCM",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "envFrom[*]",
          "configMapRef",
          "name"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraEnvVarsCM"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraEnvVarsSecret"
      },
      {
        "guards": [
          {
            "paths": [
              "extraEnvVarsCM",
              "extraEnvVarsSecret"
            ],
            "type": "or"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraEnvVarsSecret"
      },
      {
        "guards": [
          {
            "paths": [
              "extraEnvVarsCM",
              "extraEnvVarsSecret"
            ],
            "type": "or"
          },
          {
            "path": "extraEnvVarsSecret",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "envFrom[*]",
          "secretRef",
          "name"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraEnvVarsSecret"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraVolumeMounts"
      },
      {
        "guards": [
          {
            "path": "extraVolumeMounts",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "volumeMounts"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraVolumeMounts"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraVolumes"
      },
      {
        "guards": [
          {
            "path": "extraVolumes",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "volumes"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "extraVolumes"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "fourlwCommandsWhitelist"
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
        "path": [],
        "resource": null,
        "source_expr": "global"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "resources",
          "requests",
          "storage"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "global"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "resources",
          "requests",
          "storage"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "global"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "heapSize"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "hostAliases"
      },
      {
        "guards": [
          {
            "path": "hostAliases",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "hostAliases"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "hostAliases"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "image"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "image.debug"
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
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "image.pullPolicy"
      },
      {
        "guards": [
          {
            "paths": [
              "tls.client.enabled",
              "tls.quorum.enabled"
            ],
            "type": "or"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "imagePullPolicy"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "image.pullPolicy"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "initContainers"
      },
      {
        "guards": [
          {
            "path": "initContainers",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "initContainers"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "initLimit"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "jvmFlags"
      },
      {
        "guards": [
          {
            "path": "jvmFlags",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "jvmFlags"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "lifecycleHooks"
      },
      {
        "guards": [
          {
            "path": "lifecycleHooks",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "lifecycle"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "lifecycleHooks"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "listenOnAllIPs"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "logLevel"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "maxClientCnxns"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "maxSessionTimeout"
      },
      {
        "guards": [
          {
            "path": "metrics.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "metrics.containerPort"
      },
      {
        "guards": [
          {
            "path": "metrics.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "ports[*]",
          "containerPort"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "metrics.containerPort"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "metrics.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "minServerId"
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
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "namespaceOverride"
      },
      {
        "guards": [],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "affinity",
          "nodeAffinity"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "nodeAffinityPreset.key"
      },
      {
        "guards": [],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "affinity",
          "nodeAffinity"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "nodeAffinityPreset.type"
      },
      {
        "guards": [],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "affinity",
          "nodeAffinity"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "nodeAffinityPreset.values"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "nodeSelector"
      },
      {
        "guards": [
          {
            "path": "nodeSelector",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "nodeSelector"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "nodeSelector"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "resources",
          "requests",
          "storage"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "resources",
          "requests",
          "storage"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "accessModes"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.accessModes"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "accessModes"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.accessModes"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.annotations"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.annotations"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          },
          {
            "path": "persistence.annotations",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "metadata",
          "annotations"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.annotations"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          },
          {
            "path": "persistence.annotations",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "metadata",
          "annotations"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.annotations"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.dataLogDir.existingClaim"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.dataLogDir.existingClaim"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "volumes[*]",
          "persistentVolumeClaim",
          "claimName"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.dataLogDir.existingClaim"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.dataLogDir.selector"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          },
          {
            "path": "persistence.dataLogDir.selector",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "selector"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.dataLogDir.selector"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "resources",
          "requests",
          "storage"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.dataLogDir.size"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.enabled"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.enabled"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.existingClaim"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.existingClaim"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.existingClaim"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "volumes[*]",
          "persistentVolumeClaim",
          "claimName"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.existingClaim"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.labels"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.labels"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          },
          {
            "path": "persistence.labels",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "metadata",
          "labels"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.labels"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          },
          {
            "path": "persistence.labels",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "metadata",
          "labels"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.labels"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.selector"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          },
          {
            "path": "persistence.selector",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "selector"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.selector"
      },
      {
        "guards": [
          {
            "path": "persistence.dataLogDir.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "truthy"
          },
          {
            "path": "persistence.existingClaim",
            "type": "not"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "volumeClaimTemplates[*]",
          "spec",
          "resources",
          "requests",
          "storage"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "persistence.size"
      },
      {
        "guards": [],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "affinity",
          "podAffinity"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podAffinityPreset"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podAnnotations"
      },
      {
        "guards": [
          {
            "path": "podAnnotations",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "metadata",
          "annotations"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podAnnotations"
      },
      {
        "guards": [],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "affinity",
          "podAntiAffinity"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podAntiAffinityPreset"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podLabels"
      },
      {
        "guards": [
          {
            "path": "podLabels",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "metadata",
          "labels"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podLabels"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "podManagementPolicy"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podManagementPolicy"
      },
      {
        "guards": [
          {
            "path": "podSecurityContext.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "securityContext"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podSecurityContext"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podSecurityContext.enabled"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "args[*]"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podSecurityContext.fsGroup"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          },
          {
            "path": "dataLogDir",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "args[*]"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "podSecurityContext.fsGroup"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "preAllocSize"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "priorityClassName"
      },
      {
        "guards": [
          {
            "path": "priorityClassName",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "priorityClassName"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "priorityClassName"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "replicaCount"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "replicas"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "replicaCount"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "resources"
      },
      {
        "guards": [
          {
            "path": "resources",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "resources"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "resources"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "schedulerName"
      },
      {
        "guards": [
          {
            "path": "schedulerName",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "schedulerName"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "schedulerName"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "service.disableBaseClientPort"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "serviceName"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "service.headless.servicenameOverride"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "serviceAccount.create"
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
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "sidecars"
      },
      {
        "guards": [
          {
            "path": "sidecars",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "containers"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "sidecars"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "snapCount"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "syncLimit"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tickTime"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.auth"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.autoGenerated"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.enabled"
      },
      {
        "guards": [
          {
            "paths": [
              "tls.client.enabled",
              "tls.quorum.enabled"
            ],
            "type": "or"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.enabled"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "tls.client.existingSecret"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.keystorePassword"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.keystorePath"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "tls.client.passwordsSecretKeystoreKey"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "tls.client.passwordsSecretName"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.passwordsSecretName"
      },
      {
        "guards": [
          {
            "paths": [
              "tls.client.enabled",
              "tls.quorum.enabled"
            ],
            "type": "or"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.passwordsSecretName"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "tls.client.passwordsSecretTruststoreKey"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.truststorePassword"
      },
      {
        "guards": [
          {
            "path": "tls.client.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.client.truststorePath"
      },
      {
        "guards": [
          {
            "path": "tls.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.auth"
      },
      {
        "guards": [
          {
            "path": "tls.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.autoGenerated"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.enabled"
      },
      {
        "guards": [
          {
            "paths": [
              "tls.client.enabled",
              "tls.quorum.enabled"
            ],
            "type": "or"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.enabled"
      },
      {
        "guards": [
          {
            "path": "tls.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "tls.quorum.existingSecret"
      },
      {
        "guards": [
          {
            "path": "tls.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.keystorePassword"
      },
      {
        "guards": [
          {
            "path": "tls.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.keystorePath"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "tls.quorum.passwordsSecretKeystoreKey"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "tls.quorum.passwordsSecretName"
      },
      {
        "guards": [
          {
            "path": "tls.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.passwordsSecretName"
      },
      {
        "guards": [
          {
            "paths": [
              "tls.client.enabled",
              "tls.quorum.enabled"
            ],
            "type": "or"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.passwordsSecretName"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "tls.quorum.passwordsSecretTruststoreKey"
      },
      {
        "guards": [
          {
            "path": "tls.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.truststorePassword"
      },
      {
        "guards": [
          {
            "path": "tls.quorum.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "containers[*]",
          "env[*]",
          "value"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.quorum.truststorePath"
      },
      {
        "guards": [
          {
            "paths": [
              "tls.client.enabled",
              "tls.quorum.enabled"
            ],
            "type": "or"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.resources"
      },
      {
        "guards": [
          {
            "paths": [
              "tls.client.enabled",
              "tls.quorum.enabled"
            ],
            "type": "or"
          },
          {
            "path": "tls.resources",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "resources"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tls.resources"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tolerations"
      },
      {
        "guards": [
          {
            "path": "tolerations",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "tolerations"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "tolerations"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "topologySpreadConstraints"
      },
      {
        "guards": [
          {
            "path": "topologySpreadConstraints",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "topologySpreadConstraints"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "topologySpreadConstraints"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "updateStrategy"
      },
      {
        "guards": [
          {
            "path": "updateStrategy",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "updateStrategy"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "updateStrategy"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.containerSecurityContext.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "securityContext"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "volumePermissions.containerSecurityContext"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "volumePermissions.containerSecurityContext.enabled"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "volumePermissions.enabled"
      },
      {
        "guards": [],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "volumePermissions.image"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "imagePullPolicy"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "volumePermissions.image.pullPolicy"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          }
        ],
        "kind": "Scalar",
        "path": [],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "volumePermissions.resources"
      },
      {
        "guards": [
          {
            "path": "persistence.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.enabled",
            "type": "truthy"
          },
          {
            "path": "volumePermissions.resources",
            "type": "truthy"
          }
        ],
        "kind": "Fragment",
        "path": [
          "spec",
          "template",
          "spec",
          "initContainers[*]",
          "resources"
        ],
        "resource": {
          "api_version": "",
          "kind": "StatefulSet"
        },
        "source_expr": "volumePermissions.resources"
      }
    ]
        );

    similar_asserts::assert_eq!(actual, expected);
}
