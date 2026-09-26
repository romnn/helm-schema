//! Accepted documents that Helm or Kubernetes reject, found where the
//! baseline schema rejected its own chart defaults.
//!
//! Such a baseline rejects every probe exactly as it rejects the defaults,
//! so a probe it rejects carries no evidence and is judged on the candidate
//! alone. A candidate acceptance of a document Helm aborts on or the pinned
//! Kubernetes v1.29 bundle rejects is then a false acceptance the analyzer
//! already had: one the baseline hid behind its blanket rejection. Each is
//! listed here with its suspected family. The battery fails on one missing
//! from the roster and on an entry that no longer fails, which must be
//! removed.

use serde::Serialize;

/// How the render of an accepted document fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Rejection {
    /// `helm template` aborts.
    HelmAborts,
    /// Helm renders, and the pinned Kubernetes bundle rejects the render.
    KubernetesRejects,
}

/// The analyzer defect family suspected behind a false acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    /// F4: a label or annotation map read from a `toYaml` operand.
    F4,
    /// F5: `range` over a value Helm cannot iterate.
    F5,
    /// F6: bitnami validation and `fail` helpers (`common.resources.preset`,
    /// `common.errors.insecureImages`, the image pull secret helpers).
    F6,
    /// F9: a null deletion reaches a typed sink; nil is modelled, its type
    /// is not.
    F9,
    /// F13: YAML well-formedness, `toYaml` of a string where a list or map
    /// is expected, or a render that is no manifest.
    F13,
    /// F30: a numeric-looking string rendered as a plain scalar re-types.
    F30,
    /// F31: the provider type is not pushed back through a pass-through
    /// helper.
    F31,
    /// No family filed: a hard-coded API version the pinned server no
    /// longer serves, an empty `{}` item rendered as a document, a nil
    /// dereference of a deleted table.
    Unfiled,
}

/// A probe: `path` set to a value of the probe value class `value`.
pub(crate) struct Probe {
    pub(crate) path: &'static str,
    pub(crate) value: &'static str,
}

/// The known false acceptances of one chart that fail alike and share a
/// suspected family.
pub(crate) struct KnownFalseAcceptances {
    pub(crate) chart: &'static str,
    pub(crate) rejection: Rejection,
    pub(crate) family: Family,
    pub(crate) probes: &'static [Probe],
}

impl KnownFalseAcceptances {
    /// Whether the battery case `case` (`{chart}: {probe}`) is `probe` of
    /// this group, plain or as the payload of a targeted guard probe.
    pub(crate) fn names(&self, probe: &Probe, case: &str) -> bool {
        let Some(name) = case
            .strip_prefix(self.chart)
            .and_then(|rest| rest.strip_prefix(": "))
        else {
            return false;
        };
        let assignment = format!("{} <- {}", probe.path, probe.value);
        name == assignment || name.ends_with(&format!("[targeted: {assignment}]"))
    }

    /// Whether this group lists `case`, failing by `rejection`.
    pub(crate) fn lists(&self, case: &str, rejection: Rejection) -> bool {
        self.rejection == rejection && self.probes.iter().any(|probe| self.names(probe, case))
    }
}

pub(crate) const KNOWN_FALSE_ACCEPTANCES: &[KnownFalseAcceptances] = &[
    KnownFalseAcceptances {
        chart: "dify",
        rejection: Rejection::KubernetesRejects,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "dify",
        rejection: Rejection::KubernetesRejects,
        family: Family::F9,
        probes: &[Probe {
            path: "pluginDaemon.persistence.size",
            value: "null deletion [depth 3]",
        }],
    },
    KnownFalseAcceptances {
        chart: "graylog",
        rejection: Rejection::KubernetesRejects,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "graylog",
        rejection: Rejection::KubernetesRejects,
        family: Family::F31,
        probes: &[
            Probe {
                path: "graylog.affinity",
                value: "coercible string",
            },
            Probe {
                path: "graylog.affinity",
                value: "empty object item",
            },
            Probe {
                path: "graylog.affinity",
                value: "non-coercible string",
            },
            Probe {
                path: "graylog.affinity",
                value: "number",
            },
            Probe {
                path: "graylog.affinity",
                value: "true",
            },
            Probe {
                path: "graylog.affinity",
                value: "unknown object member",
            },
            Probe {
                path: "graylog.terminationGracePeriodSeconds",
                value: "non-coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "oncall",
        rejection: Rejection::HelmAborts,
        family: Family::F13,
        probes: &[
            Probe {
                path: "celery.extraContainers",
                value: "coercible string",
            },
            Probe {
                path: "celery.extraContainers",
                value: "non-coercible string",
            },
            Probe {
                path: "engine.extraContainers",
                value: "coercible string",
            },
            Probe {
                path: "engine.extraContainers",
                value: "non-coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "oncall",
        rejection: Rejection::KubernetesRejects,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::HelmAborts,
        family: Family::F13,
        probes: &[
            Probe {
                path: "extraDeploy",
                value: "unknown object member",
            },
            Probe {
                path: "extraEnvVars",
                value: "coercible string",
            },
            Probe {
                path: "extraEnvVars",
                value: "non-coercible string",
            },
            Probe {
                path: "extraVolumeMounts",
                value: "coercible string",
            },
            Probe {
                path: "extraVolumeMounts",
                value: "non-coercible string",
            },
            Probe {
                path: "extraVolumes",
                value: "coercible string",
            },
            Probe {
                path: "extraVolumes",
                value: "non-coercible string",
            },
            Probe {
                path: "image.tag",
                value: "empty string",
            },
            Probe {
                path: "networkPolicy.extraIngress",
                value: "coercible string",
            },
            Probe {
                path: "networkPolicy.extraIngress",
                value: "non-coercible string",
            },
            Probe {
                path: "service.extraPorts",
                value: "coercible string",
            },
            Probe {
                path: "service.extraPorts",
                value: "non-coercible string",
            },
            Probe {
                path: "sidecars",
                value: "coercible string",
            },
            Probe {
                path: "sidecars",
                value: "non-coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::HelmAborts,
        family: Family::F4,
        probes: &[
            Probe {
                path: "commonAnnotations",
                value: "coercible string",
            },
            Probe {
                path: "commonAnnotations",
                value: "non-coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::HelmAborts,
        family: Family::F5,
        probes: &[
            Probe {
                path: "extraDeploy",
                value: "integer",
            },
            Probe {
                path: "global.imagePullSecrets",
                value: "integer",
            },
            Probe {
                path: "image.pullSecrets",
                value: "integer",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::HelmAborts,
        family: Family::F6,
        probes: &[
            Probe {
                path: "certificates.image.registry",
                value: "null deletion [depth 3]",
            },
            Probe {
                path: "certificates.image.repository",
                value: "null deletion [depth 3]",
            },
            Probe {
                path: "global.imageRegistry",
                value: "coercible string",
            },
            Probe {
                path: "global.imageRegistry",
                value: "non-coercible string",
            },
            Probe {
                path: "global.imageRegistry",
                value: "unknown object member",
            },
            Probe {
                path: "image.registry",
                value: "coercible string",
            },
            Probe {
                path: "image.registry",
                value: "empty array",
            },
            Probe {
                path: "image.registry",
                value: "empty object",
            },
            Probe {
                path: "image.registry",
                value: "false",
            },
            Probe {
                path: "image.registry",
                value: "integer",
            },
            Probe {
                path: "image.registry",
                value: "non-coercible string",
            },
            Probe {
                path: "image.registry",
                value: "null deletion",
            },
            Probe {
                path: "image.registry",
                value: "unknown object member",
            },
            Probe {
                path: "image.repository",
                value: "coercible string",
            },
            Probe {
                path: "image.repository",
                value: "non-coercible string",
            },
            Probe {
                path: "resourcesPreset",
                value: "coercible string",
            },
            Probe {
                path: "resourcesPreset",
                value: "empty string",
            },
            Probe {
                path: "resourcesPreset",
                value: "non-coercible string",
            },
            Probe {
                path: "resourcesPreset",
                value: "null deletion",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::KubernetesRejects,
        family: Family::F30,
        probes: &[
            Probe {
                path: "extraEnvVarsCM",
                value: "coercible string",
            },
            Probe {
                path: "extraEnvVarsSecret",
                value: "coercible string",
            },
            Probe {
                path: "fullnameOverride",
                value: "coercible string",
            },
            Probe {
                path: "global.defaultStorageClass",
                value: "coercible string",
            },
            Probe {
                path: "persistence.existingClaim",
                value: "coercible string",
            },
            Probe {
                path: "persistence.storageClass",
                value: "coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::KubernetesRejects,
        family: Family::F31,
        probes: &[
            Probe {
                path: "affinity",
                value: "coercible string",
            },
            Probe {
                path: "affinity",
                value: "non-coercible string",
            },
            Probe {
                path: "args",
                value: "coercible string",
            },
            Probe {
                path: "args",
                value: "non-coercible string",
            },
            Probe {
                path: "command",
                value: "coercible string",
            },
            Probe {
                path: "command",
                value: "non-coercible string",
            },
            Probe {
                path: "containerPorts.http",
                value: "integer",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "coercible string",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "empty array",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "empty object item",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "empty string",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "false",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "integer",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "non-coercible string",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "number",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "true",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "unknown object member",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "coercible string",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "empty array",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "empty object",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "empty object item",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "empty string",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "false",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "non-coercible string",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "number",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "true",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "unknown object member",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "coercible string",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "empty array",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "empty object",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "empty object item",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "empty string",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "false",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "non-coercible string",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "number",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "true",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "unknown object member",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "coercible string",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "empty array",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "empty object item",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "empty string",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "false",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "integer",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "non-coercible string",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "number",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "true",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "unknown object member",
            },
            Probe {
                path: "customLivenessProbe",
                value: "coercible string",
            },
            Probe {
                path: "customLivenessProbe",
                value: "non-coercible string",
            },
            Probe {
                path: "customPostInitScripts",
                value: "coercible string",
            },
            Probe {
                path: "customPostInitScripts",
                value: "non-coercible string",
            },
            Probe {
                path: "customReadinessProbe",
                value: "coercible string",
            },
            Probe {
                path: "customReadinessProbe",
                value: "non-coercible string",
            },
            Probe {
                path: "customStartupProbe",
                value: "coercible string",
            },
            Probe {
                path: "customStartupProbe",
                value: "non-coercible string",
            },
            Probe {
                path: "global.imagePullSecrets",
                value: "unknown object member",
            },
            Probe {
                path: "hostAliases",
                value: "coercible string",
            },
            Probe {
                path: "hostAliases",
                value: "non-coercible string",
            },
            Probe {
                path: "image.pullSecrets",
                value: "unknown object member",
            },
            Probe {
                path: "initContainers",
                value: "coercible string",
            },
            Probe {
                path: "initContainers",
                value: "non-coercible string",
            },
            Probe {
                path: "lifecycleHooks",
                value: "coercible string",
            },
            Probe {
                path: "lifecycleHooks",
                value: "non-coercible string",
            },
            Probe {
                path: "nodeSelector",
                value: "coercible string",
            },
            Probe {
                path: "nodeSelector",
                value: "non-coercible string",
            },
            Probe {
                path: "persistence.dataSource",
                value: "coercible string",
            },
            Probe {
                path: "persistence.dataSource",
                value: "non-coercible string",
            },
            Probe {
                path: "persistence.selector",
                value: "coercible string",
            },
            Probe {
                path: "persistence.selector",
                value: "non-coercible string",
            },
            Probe {
                path: "podAnnotations",
                value: "coercible string",
            },
            Probe {
                path: "podAnnotations",
                value: "non-coercible string",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "coercible string",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "empty array",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "empty object",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "empty object item",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "empty string",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "false",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "non-coercible string",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "number",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "true",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "unknown object member",
            },
            Probe {
                path: "service.sessionAffinityConfig",
                value: "coercible string",
            },
            Probe {
                path: "service.sessionAffinityConfig",
                value: "non-coercible string",
            },
            Probe {
                path: "tolerations",
                value: "coercible string",
            },
            Probe {
                path: "tolerations",
                value: "non-coercible string",
            },
            Probe {
                path: "topologySpreadConstraints",
                value: "coercible string",
            },
            Probe {
                path: "topologySpreadConstraints",
                value: "non-coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::KubernetesRejects,
        family: Family::F9,
        probes: &[
            Probe {
                path: "persistence.size",
                value: "null deletion",
            },
            Probe {
                path: "service.ports.http",
                value: "null deletion [depth 3]",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::KubernetesRejects,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "extraDeploy",
            value: "empty object item",
        }],
    },
    KnownFalseAcceptances {
        chart: "spinnaker",
        rejection: Rejection::HelmAborts,
        family: Family::F13,
        probes: &[
            Probe {
                path: "dockerRegistries",
                value: "empty object item",
            },
            Probe {
                path: "halyard.image.tag",
                value: "null deletion [depth 3]",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "spinnaker",
        rejection: Rejection::HelmAborts,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "kubeConfig.onlySpinnakerManaged",
            value: "null deletion",
        }],
    },
    KnownFalseAcceptances {
        chart: "spinnaker",
        rejection: Rejection::KubernetesRejects,
        family: Family::Unfiled,
        probes: &[
            Probe {
                path: "ingress.enabled",
                value: "coercible string",
            },
            Probe {
                path: "ingress.enabled",
                value: "empty object item",
            },
            Probe {
                path: "ingress.enabled",
                value: "non-coercible string",
            },
            Probe {
                path: "ingress.enabled",
                value: "number",
            },
            Probe {
                path: "ingress.enabled",
                value: "true",
            },
            Probe {
                path: "ingress.enabled",
                value: "unknown object member",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "coercible string",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "empty object item",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "non-coercible string",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "number",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "true",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "unknown object member",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "coercible string",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "empty object item",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "non-coercible string",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "number",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "true",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "unknown object member",
            },
            Probe {
                path: "minio.ingress.enabled",
                value: "true",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::HelmAborts,
        family: Family::F5,
        probes: &[Probe {
            path: "extraObjects",
            value: "integer",
        }],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::KubernetesRejects,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::KubernetesRejects,
        family: Family::F9,
        probes: &[Probe {
            path: "persistence.size",
            value: "null deletion",
        }],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::KubernetesRejects,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "extraObjects",
            value: "empty object item",
        }],
    },
];
