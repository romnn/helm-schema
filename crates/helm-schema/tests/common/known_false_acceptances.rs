//! Accepted documents that Helm or Kubernetes reject, and accepted documents
//! whose render the pinned evidence cannot decide, that the battery knows.
//!
//! A baseline that rejects its own chart defaults rejects every probe exactly
//! as it rejects the defaults, so a probe it rejects carries no evidence and
//! is judged on the candidate alone. A candidate acceptance of a document
//! Helm aborts on or the pinned Kubernetes v1.29 bundle rejects is then a
//! false acceptance the analyzer already had: one the baseline hid behind its
//! blanket rejection. A baseline whose violations differ from its defaults'
//! is evidence, and a false acceptance behind it is listed only with an
//! adjudication of why that baseline rejected the cell. Each false
//! acceptance is listed with its suspected family, and each accepted
//! render Kubernetes cannot decide with the exact uncertainty the
//! adjudicator reports. The battery fails on a cell missing from the roster
//! and on an entry that no longer fails, which must be removed.

use serde::Serialize;

/// How the render of an accepted document fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Rejection {
    /// `helm template` aborts.
    HelmAborts,
    /// Helm renders, and the pinned Kubernetes bundle rejects the render.
    KubernetesRejects,
}

/// How the baseline schema rejected a false acceptance's cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Baseline {
    /// With exactly the violations of the chart's own defaults.
    RejectsItsDefaults,
    /// With violations that differ from those of the chart's own defaults.
    RejectsUnlikeItsDefaults,
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
    /// F36: a recovered type is widened with `null` and emitted without the
    /// matching `required`, so a null deletion reaches the abort.
    F36,
    /// F75: a numeric `gt` / `lt` guard in front of a `fail` produces no
    /// constraint.
    F75,
    /// No family filed: a hard-coded API version the pinned server no
    /// longer serves, an empty `{}` item rendered as a document, a nil
    /// dereference of a deleted table, a non-empty conjunct of a `kindIs`
    /// dispatch arm, a value re-parsed from a helper's `toJson` or `toYaml`
    /// output.
    Unfiled,
}

/// A probe: `path` set to a value of the probe value class `value`.
pub(crate) struct Probe {
    pub(crate) path: &'static str,
    pub(crate) value: &'static str,
}

/// Whether the battery case `case` (`{chart}: {probe}`) is `probe` of
/// `chart`, plain or as the payload of a targeted guard probe.
pub(crate) fn names(chart: &str, probe: &Probe, case: &str) -> bool {
    let Some(name) = case
        .strip_prefix(chart)
        .and_then(|rest| rest.strip_prefix(": "))
    else {
        return false;
    };
    let assignment = format!("{} <- {}", probe.path, probe.value);
    name == assignment || name.ends_with(&format!("[targeted: {assignment}]"))
}

/// The known false acceptances of one chart that fail alike behind the same
/// kind of baseline rejection and share a suspected family.
pub(crate) struct KnownFalseAcceptances {
    pub(crate) chart: &'static str,
    pub(crate) rejection: Rejection,
    pub(crate) baseline: Baseline,
    pub(crate) family: Family,
    pub(crate) probes: &'static [Probe],
}

impl KnownFalseAcceptances {
    /// Whether this group lists `case`, failing by `rejection` behind a
    /// `baseline` rejection.
    pub(crate) fn lists(&self, case: &str, rejection: Rejection, baseline: Baseline) -> bool {
        self.rejection == rejection
            && self.baseline == baseline
            && self
                .probes
                .iter()
                .any(|probe| names(self.chart, probe, case))
    }
}

/// The known accepted cells of one chart that Helm renders and whose
/// changed resources the pinned evidence cannot decide, for the same
/// reasons.
pub(crate) struct KnownUndecidedAcceptances {
    pub(crate) chart: &'static str,
    /// Every uncertainty the adjudicator reports for each listed cell,
    /// exactly and in order.
    pub(crate) uncertain: &'static [&'static str],
    pub(crate) probes: &'static [Probe],
}

impl KnownUndecidedAcceptances {
    /// Whether this group lists `case`, left undecided for exactly
    /// `uncertain`.
    pub(crate) fn lists(&self, case: &str, uncertain: &[String]) -> bool {
        self.uncertain == uncertain
            && self
                .probes
                .iter()
                .any(|probe| names(self.chart, probe, case))
    }
}

/// The baseline commit every roster row was adjudicated against. A row is
/// a cell that baseline rejected and the candidate accepts, so the battery
/// observes it only as a flip against this baseline; against any other
/// baseline the rows cannot be observed at all.
pub(crate) const ROSTER_BASELINE: &str = "f7be7ba52ba6401f527cf47ce62767343a0b1485";

pub(crate) const KNOWN_FALSE_ACCEPTANCES: &[KnownFalseAcceptances] = &[
    KnownFalseAcceptances {
        chart: "dify",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "dify",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F9,
        probes: &[Probe {
            path: "pluginDaemon.persistence.size",
            value: "null deletion [depth 3]",
        }],
    },
    KnownFalseAcceptances {
        chart: "graylog",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "graylog",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
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
    // `quote` of the deleted value renders `version:` empty (mongodb-community.yaml:9), so
    // the pinned MongoDBCommunity CRD rejects `/spec/version: null is not of type "string"`;
    // the emitted `version` is `{}`.
    KnownFalseAcceptances {
        chart: "graylog",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F9,
        probes: &[Probe {
            path: "mongodb.community.version",
            value: "null deletion [depth 3]",
        }],
    },
    KnownFalseAcceptances {
        chart: "oncall",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    // The baseline's image arms were intersected to null-only (F25/F69), so it rejects
    // the default image maps as well. `okteto.fullImage` prints the string arm bare
    // (`image: {{ include "okteto.image.backend" . }}`, webhook-deployment.yaml:86 and
    // every other image sink), so "3" renders the integer 3 and the pinned bundle
    // rejects `containers/0/image: 3 is not of types "null", "string"`; the candidate's
    // string arm admits it.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsUnlikeItsDefaults,
        family: Family::F30,
        probes: &[
            Probe {
                path: "backend.image",
                value: "coercible string",
            },
            Probe {
                path: "buildkit.image",
                value: "coercible string",
            },
            Probe {
                path: "daemonset.image",
                value: "coercible string",
            },
            Probe {
                path: "defaultBackend.image",
                value: "coercible string",
            },
            Probe {
                path: "frontend.image",
                value: "coercible string",
            },
            Probe {
                path: "installer.runner",
                value: "coercible string",
            },
            Probe {
                path: "redis.image",
                value: "coercible string",
            },
            Probe {
                path: "registry.image",
                value: "coercible string",
            },
        ],
    },
    // The baseline admitted only null here (F25/F69) and rejects the default image maps.
    // `okteto.fullImage` fails in its `else` arm on nil (_image.tpl:28, Helm: "Invalid
    // type for image value. Must be string or map."); both emitted `kindIs` arms are
    // `["null", ...]` with no `required`.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsUnlikeItsDefaults,
        family: Family::F36,
        probes: &[
            Probe {
                path: "backend.image",
                value: "null deletion",
            },
            Probe {
                path: "buildkit.image",
                value: "null deletion",
            },
            Probe {
                path: "daemonset.image",
                value: "null deletion",
            },
            Probe {
                path: "defaultBackend.image",
                value: "null deletion",
            },
            Probe {
                path: "frontend.image",
                value: "null deletion",
            },
            Probe {
                path: "installer.runner",
                value: "null deletion",
            },
            Probe {
                path: "redis.image",
                value: "null deletion",
            },
            Probe {
                path: "registry.image",
                value: "null deletion",
            },
        ],
    },
    // The baseline admitted only null here (F25/F69) and rejects the default image maps.
    // The `kindIs "string"` arm also requires `ne $image ""` (_image.tpl:13), so ""
    // reaches the `fail` (Helm: "Invalid type for image value. Must be string or map.");
    // the emitted string arm drops that conjunct. No family covers a dispatch arm's
    // non-empty conjunct.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsUnlikeItsDefaults,
        family: Family::Unfiled,
        probes: &[
            Probe {
                path: "backend.image",
                value: "empty string",
            },
            Probe {
                path: "buildkit.image",
                value: "empty string",
            },
            Probe {
                path: "daemonset.image",
                value: "empty string",
            },
            Probe {
                path: "defaultBackend.image",
                value: "empty string",
            },
            Probe {
                path: "frontend.image",
                value: "empty string",
            },
            Probe {
                path: "installer.runner",
                value: "empty string",
            },
            Probe {
                path: "redis.image",
                value: "empty string",
            },
            Probe {
                path: "registry.image",
                value: "empty string",
            },
        ],
    },
    // `toYaml` of a list under the `annotations:` map (registry-deployment.yaml:47-48)
    // is no YAML (Helm: "did not find expected key"); the emitted union admits arrays.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F13,
        probes: &[Probe {
            path: "registry.annotations",
            value: "empty object item",
        }],
    },
    // `{unknown: true}` renders the annotation value `true` (registry-deployment.yaml:48),
    // and the pinned bundle rejects `annotations/unknown: true is not of types "null",
    // "string"`; the emitted `{"type": "object"}` arm has no string member type.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F4,
        probes: &[Probe {
            path: "registry.annotations",
            value: "unknown object member",
        }],
    },
    // `range .Values.redis.args` (redis-deployment.yaml:67) aborts with "range can't
    // iterate over 0"; the emitted type admits `integer`.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F5,
        probes: &[Probe {
            path: "redis.args",
            value: "integer",
        }],
    },
    // `okteto.resourceManager.deletePeriodDays` fails when `lt (int $days) 1`
    // (_helpers.tpl:911-913, Helm: "resourceManager.deletePeriodDays must be an integer
    // greater than zero"), and `int` maps every one of these to 0; the emitted
    // `deletePeriodDays` is `{}`.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F75,
        probes: &[
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "empty array",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "empty object",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "empty object item",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "empty string",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "false",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "integer",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "non-coercible string",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "null deletion",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "unknown object member",
            },
        ],
    },
    // `okteto.clusterrolebinding` passes the value through to `roleRef.name` bare
    // (_helpers.tpl:179, okteto-cluster-admin-rbac.yaml:16), and the pinned bundle
    // rejects `/roleRef/name: <value> is not of type "string"` (a deleted value renders
    // `name:` empty, i.e. null); the emitted value carries only a description.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F31,
        probes: &[
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "empty array",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "false",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "integer",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "null deletion",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "number",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "true",
            },
        ],
    },
    // "3" reaches the bare `roleRef.name` (okteto-cluster-admin-rbac.yaml:16) as the
    // integer 3: `/roleRef/name: 3 is not of type "string"`.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "serviceAccounts.clusterRoleBinding",
            value: "coercible string",
        }],
    },
    // `[{}]` renders `name: [map[]]` at okteto-cluster-admin-rbac.yaml:16, which is no
    // YAML (Helm: "did not find expected ',' or ']'").
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F13,
        probes: &[Probe {
            path: "serviceAccounts.clusterRoleBinding",
            value: "empty object item",
        }],
    },
    // `okteto.serviceaccountsextrarolebindings` re-serializes the value with `toJson`
    // (_helpers.tpl:227), and the template ranges over its `fromJson` parse as a map of
    // lists (okteto-cluster-admin-rbac.yaml:25-29); Helm aborts with "range can't
    // iterate over json: cannot unmarshal ... into Go value of type
    // map[string]interface {}", or "range can't iterate over true" for a member. The
    // emitted value is `{}`. No family covers a value re-parsed from a helper's
    // serialized output.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "coercible string",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "empty array",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "empty object item",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "empty string",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "false",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "integer",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "non-coercible string",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "number",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "true",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "unknown object member",
            },
        ],
    },
    // `okteto.registry.storage.provider` prints `toYaml` of the value, and
    // `okteto.registry.storage.cloudProvider` re-parses it with `fromYaml` and reads
    // `$providerObj.aws.enabled` (_registry.tpl:25,32): Helm aborts with "nil pointer
    // evaluating interface {}.enabled". The emitted value is `{}`. No family covers a
    // value re-parsed from a helper's serialized output.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "registry.storage.provider",
            value: "null deletion [depth 3]",
        }],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "extraDeploy",
            value: "empty object item",
        }],
    },
    KnownFalseAcceptances {
        chart: "spinnaker",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "kubeConfig.onlySpinnakerManaged",
            value: "null deletion",
        }],
    },
    KnownFalseAcceptances {
        chart: "spinnaker",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
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
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F5,
        probes: &[Probe {
            path: "extraObjects",
            value: "integer",
        }],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F9,
        probes: &[Probe {
            path: "persistence.size",
            value: "null deletion",
        }],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "extraObjects",
            value: "empty object item",
        }],
    },
];

pub(crate) const KNOWN_UNDECIDED_ACCEPTANCES: &[KnownUndecidedAcceptances] = &[
    // A truthy `openshift.enabled` adds the `route.openshift.io/v1` Route of
    // buildkit-route.yaml:2-4; every other changed document is decided and adds
    // no violation. Route is served by the OpenShift API server, not by a
    // CRD, and the datree CRDs-catalog (ad3b08c5) publishes no
    // `route.openshift.io/route_v1.json`: its only Route schemas are the
    // OpenShift-release bundles `openshift/v4.11-strict/route_v1.json` and
    // `openshift/v4.15-strict/route_route.openshift.io_v1.json`, which the
    // catalog lookup never addresses. The render stays undecided.
    KnownUndecidedAcceptances {
        chart: "okteto",
        uncertain: &[
            "document 86: route.openshift.io/v1/Route adjudication-okteto-buildkit: \
             pinned CRD schema not found",
        ],
        probes: &[
            Probe {
                path: "openshift.enabled",
                value: "coercible string",
            },
            Probe {
                path: "openshift.enabled",
                value: "empty object item",
            },
            Probe {
                path: "openshift.enabled",
                value: "non-coercible string",
            },
            Probe {
                path: "openshift.enabled",
                value: "number",
            },
            Probe {
                path: "openshift.enabled",
                value: "true",
            },
            Probe {
                path: "openshift.enabled",
                value: "unknown object member",
            },
        ],
    },
];
