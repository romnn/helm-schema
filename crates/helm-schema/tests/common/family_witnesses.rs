//! Frozen family witnesses: the typed catalog the witness gate evaluates.
//!
//! Each campaign family owns concrete witnesses: a chart, the exact values a
//! user supplies, the verdict real Helm gave for them, and the verdict the
//! adopted schema fixture must give. The gate in `tests/family_witnesses.rs`
//! checks every row against the on-disk fixture, so a family is closed by its
//! witnesses passing, not by a mechanism commit landing.
//!
//! The rows were seeded from the independent Helm audit of 2026-09-25
//! (`round8-witness-audit/audit.md`, Helm v4.2.3). A row keeps the
//! transport it was adjudicated with: `--set` integers stay integers, while
//! numbers in a values file reach the templates as `float64`, and the two
//! can render differently.

use std::path::Path;

use color_eyre::eyre::{self, WrapErr as _};
use serde_json::{Map, Value};

use crate::known_false_acceptances::Family;

/// The families the campaign completion metric counts: F0 through F80, D4
/// and D5, as inventoried in `plan/schema-bug-hunt-v1.md`.
pub(crate) const CAMPAIGN_FAMILIES: [Family; 83] = [
    Family::F0,
    Family::F1,
    Family::F2,
    Family::F3,
    Family::F4,
    Family::F5,
    Family::F6,
    Family::F7,
    Family::F8,
    Family::F9,
    Family::F10,
    Family::F11,
    Family::F12,
    Family::F13,
    Family::F14,
    Family::F15,
    Family::F16,
    Family::F17,
    Family::F18,
    Family::F19,
    Family::F20,
    Family::F21,
    Family::F22,
    Family::F23,
    Family::F24,
    Family::F25,
    Family::F26,
    Family::F27,
    Family::F28,
    Family::F29,
    Family::F30,
    Family::F31,
    Family::F32,
    Family::F33,
    Family::F34,
    Family::F35,
    Family::F36,
    Family::F37,
    Family::F38,
    Family::F39,
    Family::F40,
    Family::F41,
    Family::F42,
    Family::F43,
    Family::F44,
    Family::F45,
    Family::F46,
    Family::F47,
    Family::F48,
    Family::F49,
    Family::F50,
    Family::F51,
    Family::F52,
    Family::F53,
    Family::F54,
    Family::F55,
    Family::F56,
    Family::F57,
    Family::F58,
    Family::F59,
    Family::F60,
    Family::F61,
    Family::F62,
    Family::F63,
    Family::F64,
    Family::F65,
    Family::F66,
    Family::F67,
    Family::F68,
    Family::F69,
    Family::F70,
    Family::F71,
    Family::F72,
    Family::F73,
    Family::F74,
    Family::F75,
    Family::F76,
    Family::F77,
    Family::F78,
    Family::F79,
    Family::F80,
    Family::D4,
    Family::D5,
];

/// The frozen witnesses of one family.
pub(crate) struct FamilyWitnesses {
    pub(crate) family: Family,
    pub(crate) witnesses: &'static [Witness],
    pub(crate) size_obligations: &'static [SizeObligation],
    /// Known witnesses of this family that are not frozen as rows yet, each
    /// with the reason. A family with any is not closed.
    pub(crate) unfrozen: &'static [&'static str],
}

/// One values document, the verdict Helm gave for it, and the verdict the
/// adopted fixture must give.
pub(crate) struct Witness {
    /// Unique across the catalog; a values file is named after it.
    pub(crate) id: &'static str,
    /// The chart directory under `testdata/charts`, and the fixture
    /// `testdata/chart-corpus-schemas/{chart}.schema.json`.
    pub(crate) chart: &'static str,
    /// The `--kube-version` Helm rendered with.
    pub(crate) kubernetes_version: &'static str,
    pub(crate) overrides: Overrides,
    pub(crate) oracle: OracleExpectation,
    pub(crate) schema: SchemaExpectation,
    /// Digest of the adjudicated inputs: chart tree, Kubernetes version,
    /// overrides and oracle. Editing any of them changes the digest, so the
    /// row fails until it is adjudicated against Helm again and the new
    /// digest recorded. Promoting `schema` needs no re-adjudication.
    pub(crate) adjudicated: &'static str,
}

/// The values a witness supplies on top of the chart defaults.
#[derive(Clone, Copy)]
pub(crate) enum Overrides {
    /// `--set key=value` pairs, applied in order. An empty list supplies
    /// nothing: the chart defaults alone.
    Set(&'static [SetPair]),
    /// `-f` with this file, relative to `tests/fixtures/family_witnesses`.
    ValuesFile(&'static str),
}

/// One `--set` pair. `key` is a dotted path of plain map keys; list indices
/// and escapes are unsupported rather than approximated.
pub(crate) struct SetPair {
    pub(crate) key: &'static str,
    pub(crate) value: SetValue,
}

/// A scalar exactly as Helm's `--set` parser types it.
#[derive(Clone, Copy)]
pub(crate) enum SetValue {
    Bool(bool),
    /// Helm keeps a `--set` integer an `int64`; a values file would make it
    /// a `float64`.
    Int(i64),
    /// A string whose spelling Helm does not re-type.
    Str(&'static str),
    /// `null`, which deletes the key.
    Null,
}

/// What real Helm did with the witness, with no schema installed.
#[derive(Clone, Copy)]
pub(crate) enum OracleExpectation {
    /// `helm template` aborts with this diagnostic.
    Aborts { diagnostic: &'static str },
    /// `helm template` renders.
    Renders { kubernetes: KubernetesExpectation },
}

/// What the pinned Kubernetes bundle says about a render.
#[derive(Clone, Copy)]
pub(crate) enum KubernetesExpectation {
    /// The witness is about rendering; resource validity is not claimed.
    NotRelevant,
    /// Every typed resource of the render validates.
    Valid,
    /// The render contains these violations.
    Invalid {
        violations: &'static [ExpectedViolation],
    },
}

/// A Kubernetes validation error found in a render.
pub(crate) struct ExpectedViolation {
    pub(crate) kind: &'static str,
    pub(crate) instance_path: &'static str,
    pub(crate) message: &'static str,
}

/// The verdict the fixture is required to give today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaExpectation {
    /// The fixture gives the verdict the oracle calls for.
    Fixed(SchemaVerdict),
    /// Known open: the oracle calls for acceptance, and the fixture still
    /// rejects.
    KnownFalseRejection,
    /// Known open: the oracle calls for rejection, and the fixture still
    /// accepts.
    KnownFalseAcceptance,
    /// No approved target: whether the schema should follow the oracle here
    /// is an open policy question. The fixture's `current` verdict is pinned
    /// so any change is seen, and the family cannot close until the policy is
    /// decided and the row becomes one of the states above.
    PolicyUnresolved {
        current: SchemaVerdict,
        question: &'static str,
    },
    /// A decided authoring policy: the oracle calls for acceptance, the
    /// default strict schema deliberately rejects, and the schema generated
    /// with the caller `option` accepts. The gate evaluates the same
    /// composed document against both fixtures. Not a fix: the default
    /// policy keeps the rejection on purpose.
    PolicyException { option: PolicyOption },
}

/// A caller authoring option that relaxes a default authoring assertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PolicyOption {
    /// `--open-root`: the chart values root admits keys no template reads.
    OpenRoot,
    /// `--declared-types=annotate`: declared defaults assert no types.
    DeclaredTypesAnnotate,
}

impl PolicyOption {
    /// The option fixture's name: `{chart}.{name}.schema.json`.
    pub(crate) fn fixture_name(self) -> &'static str {
        match self {
            Self::OpenRoot => "open-root",
            Self::DeclaredTypesAnnotate => "declared-types-annotate",
        }
    }

    /// The authoring settings an option fixture's policy annotation must
    /// record, proving it was generated under this option alone.
    pub(crate) fn authoring_annotation(self) -> Value {
        match self {
            Self::OpenRoot => serde_json::json!({"declared-types": "assert", "root": "open"}),
            Self::DeclaredTypesAnnotate => {
                serde_json::json!({"declared-types": "annotate", "root": "closed"})
            }
        }
    }
}

/// A schema verdict on one values document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SchemaVerdict {
    Accepts,
    Rejects,
}

/// Not a verdict: the schema Helm reads for `chart`'s fixture (compact, with
/// short `$defs` keys, as `helm-schema lint` / `template` ship it) must fit
/// Helm's file-size limit (`HELM_MAX_CHART_FILE_BYTES`,
/// inclusive), or Helm refuses the chart before any values are checked.
pub(crate) struct SizeObligation {
    pub(crate) id: &'static str,
    pub(crate) chart: &'static str,
    pub(crate) expectation: ObligationExpectation,
}

/// The state an obligation is required to be in today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObligationExpectation {
    Met,
    /// Known open: the obligation is still violated.
    KnownUnmet,
}

impl OracleExpectation {
    /// The verdict the oracle calls for: reject what Helm aborts on or what
    /// renders into invalid resources, accept every other render. This is
    /// the schema's target unless the row's policy is unresolved.
    pub(crate) fn desired(&self) -> SchemaVerdict {
        match self {
            Self::Aborts { .. }
            | Self::Renders {
                kubernetes: KubernetesExpectation::Invalid { .. },
            } => SchemaVerdict::Rejects,
            Self::Renders { .. } => SchemaVerdict::Accepts,
        }
    }
}

impl Overrides {
    /// The sparse values document these overrides supply; the gate hands it
    /// to the coalescer, which composes it over the chart defaults.
    ///
    /// # Errors
    ///
    /// Returns an error when a values file cannot be read or is not a map,
    /// or when a `--set` path descends through a value an earlier pair set.
    pub(crate) fn overlay(self, values_files: &Path) -> eyre::Result<Value> {
        match self {
            Self::Set(pairs) => {
                let mut root = Map::new();
                for pair in pairs {
                    let mut segments: Vec<&str> = pair.key.split('.').collect();
                    let last = segments.pop().unwrap_or_default();
                    let mut node = &mut root;
                    for segment in segments {
                        let child = node
                            .entry(segment)
                            .or_insert_with(|| Value::Object(Map::new()));
                        let Value::Object(child) = child else {
                            eyre::bail!("--set {} descends through a scalar", pair.key);
                        };
                        node = child;
                    }
                    node.insert(last.to_string(), pair.value.to_json());
                }
                Ok(Value::Object(root))
            }
            Self::ValuesFile(relative) => {
                let path = values_files.join(relative);
                let source = std::fs::read_to_string(&path)
                    .wrap_err_with(|| format!("read {}", path.display()))?;
                let values: Value = serde_yaml::from_str(&source)
                    .wrap_err_with(|| format!("parse {}", path.display()))?;
                eyre::ensure!(values.is_object(), "{} is not a map", path.display());
                Ok(values)
            }
        }
    }
}

impl SetPair {
    /// The `--set` argument Helm receives.
    pub(crate) fn argument(&self) -> String {
        format!("{}={}", self.key, self.value.spelling())
    }
}

impl SetValue {
    fn to_json(self) -> Value {
        match self {
            Self::Bool(value) => Value::Bool(value),
            Self::Int(value) => Value::from(value),
            Self::Str(value) => Value::from(value),
            Self::Null => Value::Null,
        }
    }

    fn spelling(self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Str(value) => value.to_string(),
            Self::Null => "null".to_string(),
        }
    }
}

const F1_POLICY: &str = "must a closed root admit the `global` map Helm injects into a chart \
     that never reads it?";

const RENDERS: OracleExpectation = OracleExpectation::Renders {
    kubernetes: KubernetesExpectation::NotRelevant,
};

pub(crate) const FAMILY_WITNESSES: &[FamilyWitnesses] = &[
    FamilyWitnesses {
        family: Family::F1,
        witnesses: &[
            Witness {
                id: "aws-load-balancer-controller-global-image-registry",
                chart: "aws-load-balancer-controller",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F1/aws-load-balancer-controller-global-image-registry.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "16d664313cce2082",
            },
            Witness {
                id: "cluster-autoscaler-global-image-registry",
                chart: "cluster-autoscaler",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F1/cluster-autoscaler-global-image-registry.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "a0720c89b9f16e81",
            },
            Witness {
                id: "common-global-image-registry",
                chart: "common",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/common-global-image-registry.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "697459542595eb30",
            },
            Witness {
                id: "dex-global-image-registry",
                chart: "dex",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/dex-global-image-registry.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "3b2430a5e8490fc1",
            },
            Witness {
                id: "harbor-global-image-registry",
                chart: "harbor",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/harbor-global-image-registry.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "32943d998bfbe134",
            },
            Witness {
                id: "karpenter-global-image-registry",
                chart: "karpenter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/karpenter-global-image-registry.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "2369164374809812",
            },
            Witness {
                id: "kubeview-global-image-registry",
                chart: "kubeview",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/kubeview-global-image-registry.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "2209cc3b511c5b9b",
            },
            Witness {
                id: "nginx-ingress-global-image-registry",
                chart: "nginx-ingress",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/nginx-ingress-global-image-registry.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "89239c69074123a2",
            },
            Witness {
                id: "zalando-postgres-operator-global-image-registry",
                chart: "zalando-postgres-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F1/zalando-postgres-operator-global-image-registry.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "a9e67ffa5bc8d1bb",
            },
            Witness {
                id: "zalando-postgres-operator-ui-global-image-registry",
                chart: "zalando-postgres-operator-ui",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F1/zalando-postgres-operator-ui-global-image-registry.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: F1_POLICY,
                },
                adjudicated: "b1ff9fbe794fa818",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F4,
        witnesses: &[Witness {
            id: "synapse-defaults",
            chart: "synapse",
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(&[]),
            oracle: RENDERS,
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "b40c117912a3e669",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F5,
        witnesses: &[
            Witness {
                id: "grafana-set-extra-objects-int-minus-1",
                chart: "grafana",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "extraObjects",
                    value: SetValue::Int(-1),
                }]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "449d45f23c8d94ef",
            },
            Witness {
                id: "grafana-set-extra-objects-int-0",
                chart: "grafana",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "extraObjects",
                    value: SetValue::Int(0),
                }]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "c4998899657b4145",
            },
            Witness {
                id: "grafana-set-extra-objects-int-5",
                chart: "grafana",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "extraObjects",
                    value: SetValue::Int(5),
                }]),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: YAML parse error on grafana/templates/extra-manifests.yaml: error unmarshaling JSON: while decoding JSON: json: cannot unmarshal number into Go value of type util.SimpleHead",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "2a9ba02632234c6c",
            },
            Witness {
                id: "grafana-values-extra-objects-int-minus-1",
                chart: "grafana",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F5/grafana-values-extra-objects-int-minus-1.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: grafana/templates/extra-manifests.yaml:1:16 — range can't iterate over -1",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "6708081b6978d58a",
            },
            Witness {
                id: "grafana-values-extra-objects-int-0",
                chart: "grafana",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F5/grafana-values-extra-objects-int-0.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: grafana/templates/extra-manifests.yaml:1:16 — range can't iterate over 0",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "c35b909b76dd7b7c",
            },
            Witness {
                id: "jaeger-set-extra-objects-int-minus-1",
                chart: "jaeger",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "extraObjects",
                    value: SetValue::Int(-1),
                }]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "99d17e7179bcc51c",
            },
            Witness {
                id: "jaeger-set-extra-objects-int-0",
                chart: "jaeger",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "extraObjects",
                    value: SetValue::Int(0),
                }]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "2f16bdff5db587b8",
            },
            Witness {
                id: "jaeger-set-extra-objects-int-5",
                chart: "jaeger",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "extraObjects",
                    value: SetValue::Int(5),
                }]),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: YAML parse error on jaeger/templates/jaeger/jaeger-extra-list.yaml: error unmarshaling JSON: while decoding JSON: json: cannot unmarshal number into Go value of type util.SimpleHead",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "9818de6cce79b87a",
            },
            Witness {
                id: "jaeger-values-extra-objects-int-minus-1",
                chart: "jaeger",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F5/jaeger-values-extra-objects-int-minus-1.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: jaeger/templates/jaeger/jaeger-extra-list.yaml:1:17 — range can't iterate over -1",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "09b5212fb9ede737",
            },
            Witness {
                id: "jaeger-values-extra-objects-int-0",
                chart: "jaeger",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F5/jaeger-values-extra-objects-int-0.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: jaeger/templates/jaeger/jaeger-extra-list.yaml:1:17 — range can't iterate over 0",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "d02288825e1a26f1",
            },
            Witness {
                id: "opensearch-set-secret-mounts-int-minus-1",
                chart: "opensearch",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "secretMounts",
                    value: SetValue::Int(-1),
                }]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "db12fa03f1ead900",
            },
            Witness {
                id: "opensearch-set-secret-mounts-int-0",
                chart: "opensearch",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "secretMounts",
                    value: SetValue::Int(0),
                }]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "967c7d0c74bb37e6",
            },
            Witness {
                id: "opensearch-set-secret-mounts-int-5",
                chart: "opensearch",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "secretMounts",
                    value: SetValue::Int(5),
                }]),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: opensearch/templates/statefulset.yaml:172:17 — can't evaluate field name in type int64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "a88375c0420bd22a",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F6,
        witnesses: &[Witness {
            id: "bitnami-redis-architecture-bogus",
            chart: "bitnami-redis",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F6/bitnami-redis-architecture-bogus.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: execution error at (redis/templates/NOTES.txt:202:4): VALUES VALIDATION:",
            },
            schema: SchemaExpectation::KnownFalseAcceptance,
            adjudicated: "509be2d567f6bbd8",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F9,
        witnesses: &[Witness {
            id: "cilium-extra-config-string",
            chart: "cilium",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F9/cilium-extra-config-string.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: template: cilium/templates/validate.yaml:165:9: executing \"cilium/templates/validate.yaml\" at <index .Values.extraConfig \"allow-unsafe-policy-skb-usage\">: error calling index: cannot index slice/array with type string",
            },
            schema: SchemaExpectation::KnownFalseAcceptance,
            adjudicated: "7c6bac5f9751aabf",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F13,
        witnesses: &[Witness {
            id: "nats-operator-image-tag-null",
            chart: "nats-operator",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F13/nats-operator-image-tag-null.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: YAML parse error on nats-operator/templates/deployment.yaml: error converting YAML to JSON: yaml: line 40: mapping values are not allowed in this context",
            },
            schema: SchemaExpectation::KnownFalseAcceptance,
            adjudicated: "e7e60310ab316daf",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F17,
        witnesses: &[
            Witness {
                id: "nats-container-env-map",
                chart: "nats",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F17/nats-container-env-map.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "1c62a4b69bd44e2c",
            },
            Witness {
                id: "nats-nats-box-env-map",
                chart: "nats",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F17/nats-nats-box-env-map.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "7315b4685d758c96",
            },
            Witness {
                id: "nats-prom-exporter-env-map",
                chart: "nats",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F17/nats-prom-exporter-env-map.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "0523915223bb0294",
            },
            Witness {
                id: "nats-reloader-env-map",
                chart: "nats",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F17/nats-reloader-env-map.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "ee7183146a0702f8",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F23,
        witnesses: &[Witness {
            id: "rook-ceph-controller-manager-null",
            chart: "rook-ceph",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F23/rook-ceph-controller-manager-null.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: rook-ceph/charts/ceph-csi-operator/templates/deployment.yaml:11:22 — nil pointer evaluating interface {}.replicas",
            },
            schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
            adjudicated: "5225b2bd69d2ff96",
        }],
        size_obligations: &[],
        unfrozen: &[
            "oncall `postgresql.auth.secretKeys.adminPasswordKey=null`: Helm renders without the schema, and with the shipped (compact, 3,658,802-byte) schema Helm refuses the schema itself: its `rabbitmq.ldap.uri` `pattern` is invalid under Go regexp. The jsonschema crate compiles that pattern, so the verdict is not reproducible offline.",
        ],
    },
    FamilyWitnesses {
        family: Family::F30,
        witnesses: &[Witness {
            id: "kubeshark-cloud-license-enabled-false",
            chart: "kubeshark",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F30/kubeshark-cloud-license-enabled-false.yaml"),
            oracle: RENDERS,
            schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
            adjudicated: "d4ab3f29ae2af563",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F31,
        witnesses: &[
            Witness {
                id: "argo-events-controller-rbac-rules-missing-verbs",
                chart: "argo-events",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F31/argo-events-controller-rbac-rules-missing-verbs.yaml",
                ),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Invalid {
                        violations: &[ExpectedViolation {
                            kind: "ClusterRole",
                            instance_path: "/rules/0",
                            message: "missing verbs",
                        }],
                    },
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "b79f64da724a1b27",
            },
            Witness {
                id: "external-secrets-topology-spread-constraints-string-item",
                chart: "external-secrets",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F31/external-secrets-topology-spread-constraints-string-item.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: external-secrets/templates/deployment.yaml:222:32 — can't evaluate field labelSelector in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "29249d915a052bbf",
            },
            Witness {
                id: "kubeshark-tap-security-context-privileged-false",
                chart: "kubeshark",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F31/kubeshark-tap-security-context-privileged-false.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "efaa31e5e063478f",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F40,
        witnesses: &[Witness {
            id: "jupyterhub-hub-image-tag-empty",
            chart: "jupyterhub",
            kubernetes_version: "1.33.0",
            overrides: Overrides::ValuesFile("F40/jupyterhub-hub-image-tag-empty.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: YAML parse error on jupyterhub/templates/hub/deployment.yaml: error converting YAML to JSON: yaml: line 77: mapping values are not allowed in this context",
            },
            schema: SchemaExpectation::KnownFalseAcceptance,
            adjudicated: "821f796824fbfc8e",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F51,
        witnesses: &[Witness {
            id: "eck-stack-defaults",
            chart: "eck-stack",
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(&[]),
            oracle: RENDERS,
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "0872e2f2a26008b9",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F54,
        witnesses: &[
            Witness {
                id: "promtail-set-cidrs-empty-string",
                chart: "promtail",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[
                    SetPair {
                        key: "networkPolicy.enabled",
                        value: SetValue::Bool(true),
                    },
                    SetPair {
                        key: "networkPolicy.k8sApi.cidrs",
                        value: SetValue::Str(""),
                    },
                ]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "bfa3988985e95622",
            },
            Witness {
                id: "promtail-set-cidrs-int-3",
                chart: "promtail",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[
                    SetPair {
                        key: "networkPolicy.enabled",
                        value: SetValue::Bool(true),
                    },
                    SetPair {
                        key: "networkPolicy.k8sApi.cidrs",
                        value: SetValue::Int(3),
                    },
                ]),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: template: promtail/templates/networkpolicy.yaml:59:13: executing \"promtail/templates/networkpolicy.yaml\" at <len .Values.networkPolicy.k8sApi.cidrs>: error calling len: len of type int64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "b8370fcf212fb354",
            },
            Witness {
                id: "promtail-set-cidrs-true",
                chart: "promtail",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[
                    SetPair {
                        key: "networkPolicy.enabled",
                        value: SetValue::Bool(true),
                    },
                    SetPair {
                        key: "networkPolicy.k8sApi.cidrs",
                        value: SetValue::Bool(true),
                    },
                ]),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: template: promtail/templates/networkpolicy.yaml:59:13: executing \"promtail/templates/networkpolicy.yaml\" at <len .Values.networkPolicy.k8sApi.cidrs>: error calling len: len of type bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "30089c54688ba952",
            },
            Witness {
                id: "promtail-set-cidrs-string",
                chart: "promtail",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[
                    SetPair {
                        key: "networkPolicy.enabled",
                        value: SetValue::Bool(true),
                    },
                    SetPair {
                        key: "networkPolicy.k8sApi.cidrs",
                        value: SetValue::Str("xyz"),
                    },
                ]),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: promtail/templates/networkpolicy.yaml:61:34 — range can't iterate over xyz",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "5c9ee05e01c955ef",
            },
            Witness {
                id: "promtail-values-cidrs-null",
                chart: "promtail",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F54/promtail-values-cidrs-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: template: promtail/templates/networkpolicy.yaml:59:13: executing \"promtail/templates/networkpolicy.yaml\" at <len .Values.networkPolicy.k8sApi.cidrs>: error calling len: len of nil pointer",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "ca54e909aa3ea830",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F69,
        witnesses: &[
            Witness {
                id: "cilium-clusters-null",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F69/cilium-clusters-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (cilium/templates/clustermesh-config/clustermesh-secret.yaml:21:13): unknown type invalid for clustermesh.config.clusters",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "1cf5840736cb8ba8",
            },
            Witness {
                id: "cilium-clusters-empty-list",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F69/cilium-clusters-empty-list.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "932c2cb785661ca5",
            },
            Witness {
                id: "cilium-clusters-empty-map",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F69/cilium-clusters-empty-map.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "af54049216a4bceb",
            },
            Witness {
                id: "cilium-clustermesh-enabled",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F69/cilium-clustermesh-enabled.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "664ee9c5f6284236",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F73,
        witnesses: &[
            Witness {
                id: "datadog-ci-security-agent-compliance",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F73/datadog-ci-security-agent-compliance.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyException {
                    option: PolicyOption::OpenRoot,
                },
                adjudicated: "98ab313bb0fce144",
            },
            Witness {
                id: "datadog-root-arbitrary-key",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F73/datadog-root-arbitrary-key.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyException {
                    option: PolicyOption::OpenRoot,
                },
                adjudicated: "0027ffd75b4449f0",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F74,
        witnesses: &[],
        size_obligations: &[
            SizeObligation {
                id: "airflow-fixture-size",
                chart: "airflow",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "datadog-fixture-size",
                chart: "datadog",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "dify-fixture-size",
                chart: "dify",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "gitea-fixture-size",
                chart: "gitea",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "kube-prometheus-stack-fixture-size",
                chart: "kube-prometheus-stack",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "kyverno-fixture-size",
                chart: "kyverno",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "milvus-fixture-size",
                chart: "milvus",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "nats-fixture-size",
                chart: "nats",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "netbox-fixture-size",
                chart: "netbox",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "okteto-fixture-size",
                chart: "okteto",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "oncall-fixture-size",
                chart: "oncall",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "openebs-fixture-size",
                chart: "openebs",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "prometheus-fixture-size",
                chart: "prometheus",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "redmine-fixture-size",
                chart: "redmine",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "signoz-signoz-fixture-size",
                chart: "signoz-signoz",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "stacks-blockchain-api-fixture-size",
                chart: "stacks-blockchain-api",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "synapse-fixture-size",
                chart: "synapse",
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "weblate-fixture-size",
                chart: "weblate",
                expectation: ObligationExpectation::Met,
            },
        ],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F77,
        witnesses: &[
            Witness {
                id: "kubernetes-event-exporter-image-repository-int-1",
                chart: "kubernetes-event-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/kubernetes-event-exporter-image-repository-int-1.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kubernetes-event-exporter/templates/NOTES.txt:39:4 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "9126d8a2bb35adbe",
            },
            Witness {
                id: "kubernetes-event-exporter-image-repository-float",
                chart: "kubernetes-event-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/kubernetes-event-exporter-image-repository-float.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kubernetes-event-exporter/templates/NOTES.txt:39:4 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "405adba2a2a793cb",
            },
            Witness {
                id: "kubernetes-event-exporter-image-repository-true",
                chart: "kubernetes-event-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/kubernetes-event-exporter-image-repository-true.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kubernetes-event-exporter/templates/NOTES.txt:39:4 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "2b4a152c2a8fc53b",
            },
            Witness {
                id: "kubernetes-event-exporter-image-repository-empty-list",
                chart: "kubernetes-event-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/kubernetes-event-exporter-image-repository-empty-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kubernetes-event-exporter/templates/NOTES.txt:39:4 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "ebdb546e0a1bfd3c",
            },
            Witness {
                id: "kubernetes-event-exporter-image-repository-object-item-list",
                chart: "kubernetes-event-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/kubernetes-event-exporter-image-repository-object-item-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kubernetes-event-exporter/templates/NOTES.txt:39:4 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "25ffdce6430cabf4",
            },
            Witness {
                id: "kubernetes-event-exporter-image-repository-false",
                chart: "kubernetes-event-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/kubernetes-event-exporter-image-repository-false.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kubernetes-event-exporter/templates/NOTES.txt:39:4 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "7a8c000af8ffa3fc",
            },
            Witness {
                id: "kubernetes-event-exporter-image-repository-unknown-member",
                chart: "kubernetes-event-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/kubernetes-event-exporter-image-repository-unknown-member.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kubernetes-event-exporter/templates/NOTES.txt:39:4 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "c6c1682a918615bd",
            },
            Witness {
                id: "kubernetes-event-exporter-image-repository-empty-map",
                chart: "kubernetes-event-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/kubernetes-event-exporter-image-repository-empty-map.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kubernetes-event-exporter/templates/NOTES.txt:39:4 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "22dfa071170867d8",
            },
            Witness {
                id: "phpmyadmin-image-repository-int-1",
                chart: "phpmyadmin",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/phpmyadmin-image-repository-int-1.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: phpmyadmin/templates/NOTES.txt:63:4 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "bf190f000cad7cd6",
            },
            Witness {
                id: "phpmyadmin-image-repository-float",
                chart: "phpmyadmin",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/phpmyadmin-image-repository-float.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: phpmyadmin/templates/NOTES.txt:63:4 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "ab74e471bb92e0e3",
            },
            Witness {
                id: "phpmyadmin-image-repository-true",
                chart: "phpmyadmin",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/phpmyadmin-image-repository-true.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: phpmyadmin/templates/NOTES.txt:63:4 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "b8303eccd7967891",
            },
            Witness {
                id: "phpmyadmin-image-repository-empty-list",
                chart: "phpmyadmin",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/phpmyadmin-image-repository-empty-list.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: phpmyadmin/templates/NOTES.txt:63:4 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "ab8739b60eb2039a",
            },
            Witness {
                id: "phpmyadmin-image-repository-object-item-list",
                chart: "phpmyadmin",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/phpmyadmin-image-repository-object-item-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: phpmyadmin/templates/NOTES.txt:63:4 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "ee2134490291c4b7",
            },
            Witness {
                id: "phpmyadmin-image-repository-false",
                chart: "phpmyadmin",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/phpmyadmin-image-repository-false.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: phpmyadmin/templates/NOTES.txt:63:4 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "1b78bf14d3390924",
            },
            Witness {
                id: "phpmyadmin-image-repository-unknown-member",
                chart: "phpmyadmin",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/phpmyadmin-image-repository-unknown-member.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: phpmyadmin/templates/NOTES.txt:63:4 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "0f2a2c0553657642",
            },
            Witness {
                id: "phpmyadmin-image-repository-empty-map",
                chart: "phpmyadmin",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/phpmyadmin-image-repository-empty-map.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: phpmyadmin/templates/NOTES.txt:63:4 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "8ab27e22dd455287",
            },
            Witness {
                id: "rabbitmq-cluster-operator-credential-updater-image-repository-int-1",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-credential-updater-image-repository-int-1.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:53:3 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "7d765941c6dd4bd7",
            },
            Witness {
                id: "rabbitmq-cluster-operator-credential-updater-image-repository-float",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-credential-updater-image-repository-float.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:53:3 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "7d68f47dea5d6ffa",
            },
            Witness {
                id: "rabbitmq-cluster-operator-credential-updater-image-repository-true",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-credential-updater-image-repository-true.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:53:3 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "4b5969948779ef79",
            },
            Witness {
                id: "rabbitmq-cluster-operator-credential-updater-image-repository-empty-list",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-credential-updater-image-repository-empty-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:53:3 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "63f1885a902ea2b2",
            },
            Witness {
                id: "rabbitmq-cluster-operator-credential-updater-image-repository-object-item-list",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-credential-updater-image-repository-object-item-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:53:3 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "24c70d856b416009",
            },
            Witness {
                id: "rabbitmq-cluster-operator-credential-updater-image-repository-false",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-credential-updater-image-repository-false.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:53:3 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "2402dc1ee3498b2a",
            },
            Witness {
                id: "rabbitmq-cluster-operator-credential-updater-image-repository-unknown-member",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-credential-updater-image-repository-unknown-member.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:53:3 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "6d029e9fa5253f99",
            },
            Witness {
                id: "rabbitmq-cluster-operator-credential-updater-image-repository-empty-map",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-credential-updater-image-repository-empty-map.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:53:3 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "6cc22a9250b1d454",
            },
            Witness {
                id: "rabbitmq-cluster-operator-rabbitmq-image-repository-int-1",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-rabbitmq-image-repository-int-1.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:54:3 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "6ca60ea04c197842",
            },
            Witness {
                id: "rabbitmq-cluster-operator-rabbitmq-image-repository-float",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-rabbitmq-image-repository-float.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:54:3 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "a2c9610241e8e08f",
            },
            Witness {
                id: "rabbitmq-cluster-operator-rabbitmq-image-repository-true",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-rabbitmq-image-repository-true.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:54:3 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "f9b2cbdf3da2bc89",
            },
            Witness {
                id: "rabbitmq-cluster-operator-rabbitmq-image-repository-empty-list",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-rabbitmq-image-repository-empty-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:54:3 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "5fdcb6de10f82905",
            },
            Witness {
                id: "rabbitmq-cluster-operator-rabbitmq-image-repository-object-item-list",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-rabbitmq-image-repository-object-item-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:54:3 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "8906dfa012eb64bb",
            },
            Witness {
                id: "rabbitmq-cluster-operator-rabbitmq-image-repository-false",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-rabbitmq-image-repository-false.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:54:3 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "0f08b26e64d21229",
            },
            Witness {
                id: "rabbitmq-cluster-operator-rabbitmq-image-repository-unknown-member",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-rabbitmq-image-repository-unknown-member.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:54:3 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "853626dcade8b19d",
            },
            Witness {
                id: "rabbitmq-cluster-operator-rabbitmq-image-repository-empty-map",
                chart: "rabbitmq-cluster-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/rabbitmq-cluster-operator-rabbitmq-image-repository-empty-map.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: rabbitmq-cluster-operator/templates/NOTES.txt:54:3 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "cd03d6cf1994baf7",
            },
            Witness {
                id: "zookeeper-image-repository-int-1",
                chart: "zookeeper",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/zookeeper-image-repository-int-1.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: zookeeper/templates/NOTES.txt:80:4 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "86eb73af5e0223f7",
            },
            Witness {
                id: "zookeeper-image-repository-float",
                chart: "zookeeper",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/zookeeper-image-repository-float.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: zookeeper/templates/NOTES.txt:80:4 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "28f480a7af591422",
            },
            Witness {
                id: "zookeeper-image-repository-true",
                chart: "zookeeper",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/zookeeper-image-repository-true.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: zookeeper/templates/NOTES.txt:80:4 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "20135a78f9e0e4eb",
            },
            Witness {
                id: "zookeeper-image-repository-empty-list",
                chart: "zookeeper",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/zookeeper-image-repository-empty-list.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: zookeeper/templates/NOTES.txt:80:4 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "9355c2e4505593fc",
            },
            Witness {
                id: "zookeeper-image-repository-object-item-list",
                chart: "zookeeper",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/zookeeper-image-repository-object-item-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: zookeeper/templates/NOTES.txt:80:4 — wrong type for value; expected string; got []interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "85e0b097fe424999",
            },
            Witness {
                id: "zookeeper-image-repository-false",
                chart: "zookeeper",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/zookeeper-image-repository-false.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: zookeeper/templates/NOTES.txt:80:4 — wrong type for value; expected string; got bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "d70cfddc92a05890",
            },
            Witness {
                id: "zookeeper-image-repository-unknown-member",
                chart: "zookeeper",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F77/zookeeper-image-repository-unknown-member.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: zookeeper/templates/NOTES.txt:80:4 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "b3ad41a16de8e893",
            },
            Witness {
                id: "zookeeper-image-repository-empty-map",
                chart: "zookeeper",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F77/zookeeper-image-repository-empty-map.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: zookeeper/templates/NOTES.txt:80:4 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "96377b3ff6e93f92",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F78,
        witnesses: &[
            Witness {
                id: "datadog-operator-datadog-agent-empty-string",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F78/datadog-operator-datadog-agent-empty-string.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/charts/operator/templates/deployment.yaml:178:46 — can't evaluate field enabled in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "1fb620ef04c408bb",
            },
            Witness {
                id: "datadog-operator-datadog-agent-numeric-string",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F78/datadog-operator-datadog-agent-numeric-string.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/charts/operator/templates/deployment.yaml:178:46 — can't evaluate field enabled in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "a86c3802f524e13e",
            },
            Witness {
                id: "datadog-operator-datadog-agent-string",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F78/datadog-operator-datadog-agent-string.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/charts/operator/templates/deployment.yaml:178:46 — can't evaluate field enabled in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "aa75b80c3d2bff03",
            },
            Witness {
                id: "datadog-operator-datadog-agent-int-1",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F78/datadog-operator-datadog-agent-int-1.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/charts/operator/templates/deployment.yaml:178:46 — can't evaluate field enabled in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "ec10f613f9269a04",
            },
            Witness {
                id: "datadog-operator-datadog-agent-float",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F78/datadog-operator-datadog-agent-float.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/charts/operator/templates/deployment.yaml:178:46 — can't evaluate field enabled in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "3a5ae0243e0dbb62",
            },
            Witness {
                id: "datadog-operator-datadog-agent-false",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F78/datadog-operator-datadog-agent-false.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/charts/operator/templates/deployment.yaml:178:46 — can't evaluate field enabled in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "4acf915a389e63f5",
            },
            Witness {
                id: "datadog-operator-datadog-agent-true",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F78/datadog-operator-datadog-agent-true.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/charts/operator/templates/deployment.yaml:178:46 — can't evaluate field enabled in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "09fd3549a01a0175",
            },
            Witness {
                id: "datadog-operator-datadog-agent-empty-list",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F78/datadog-operator-datadog-agent-empty-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/charts/operator/templates/deployment.yaml:178:46 — can't evaluate field enabled in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "776b4b147099e683",
            },
            Witness {
                id: "datadog-operator-datadog-agent-object-item-list",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F78/datadog-operator-datadog-agent-object-item-list.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/charts/operator/templates/deployment.yaml:178:46 — can't evaluate field enabled in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "c6eb2293335a081c",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F79,
        witnesses: &[Witness {
            id: "oauth2-proxy-config-existing-config-true",
            chart: "oauth2-proxy",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F79/oauth2-proxy-config-existing-config-true.yaml"),
            oracle: OracleExpectation::Renders {
                kubernetes: KubernetesExpectation::Invalid {
                    violations: &[ExpectedViolation {
                        kind: "Deployment",
                        instance_path: "/spec/template/spec/volumes/0/configMap/name",
                        message: "got boolean, want null or string",
                    }],
                },
            },
            schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
            adjudicated: "d03d38b21351c8a2",
        }],
        size_obligations: &[],
        unfrozen: &[
            "F79 is an adjudicator defect: the round-74 oracle files a Kubernetes-typed rejection as a false rejection. The oauth2-proxy row checks only the schema verdict; closure needs a classification obligation that runs the adjudicator on the witness and confirms it classifies the rejection as a Kubernetes rejection (D4).",
        ],
    },
    FamilyWitnesses {
        family: Family::F80,
        witnesses: &[
            Witness {
                id: "redis-ha-haproxy-check-fall-string",
                chart: "redis-ha",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F80/redis-ha-haproxy-check-fall-string.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyException {
                    option: PolicyOption::DeclaredTypesAnnotate,
                },
                adjudicated: "dd20b558fe75737e",
            },
            Witness {
                id: "redis-ha-haproxy-check-interval-int",
                chart: "redis-ha",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F80/redis-ha-haproxy-check-interval-int.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::PolicyException {
                    option: PolicyOption::DeclaredTypesAnnotate,
                },
                adjudicated: "aefa91b204944273",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::D5,
        witnesses: &[
            Witness {
                id: "kube-starrocks-defaults",
                chart: "kube-starrocks",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "0ea9d5501f403d0f",
            },
            Witness {
                id: "openldap-stack-ha-defaults",
                chart: "openldap-stack-ha",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "902d9744e90e8374",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::D3,
        witnesses: &[Witness {
            id: "milvus-defaults",
            chart: "milvus",
            kubernetes_version: "1.29.0",
            overrides: Overrides::Set(&[]),
            oracle: RENDERS,
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "ebc81bed15a8868c",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::B6,
        witnesses: &[
            Witness {
                id: "influxdb-set-hostname-int-0",
                chart: "influxdb",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "ingress.hostname",
                    value: SetValue::Int(0),
                }]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "88deed4025ba59b9",
            },
            Witness {
                id: "influxdb-set-hostname-null",
                chart: "influxdb",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[SetPair {
                    key: "ingress.hostname",
                    value: SetValue::Null,
                }]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "82ffdccf8cae855a",
            },
            Witness {
                id: "nginx-hostname-null-enabled",
                chart: "nginx",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("B6/nginx-hostname-null-enabled.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: nginx/templates/NOTES.txt:51:79 — wrong type for value; expected string; got interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "14209edbc2e5dc2a",
            },
            Witness {
                id: "nginx-hostname-true-disabled",
                chart: "nginx",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("B6/nginx-hostname-true-disabled.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "5fc12a1f2ca93ece",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::L1,
        witnesses: &[
            Witness {
                id: "loki-swift-connect-timeout-5",
                chart: "loki",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("L1/loki-swift-connect-timeout-5.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "f3fe1d683e230c17",
            },
            Witness {
                id: "loki-swift-max-retries-abc",
                chart: "loki",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("L1/loki-swift-max-retries-abc.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "bf83d11e9c985662",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::L2,
        witnesses: &[Witness {
            id: "datadog-trace-agent-capabilities-add",
            chart: "datadog",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("L2/datadog-trace-agent-capabilities-add.yaml"),
            oracle: OracleExpectation::Renders {
                kubernetes: KubernetesExpectation::Valid,
            },
            schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
            adjudicated: "d13b747f6c6e6190",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::Unfiled,
        witnesses: &[
            Witness {
                id: "datadog-defaults",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "f64ccc0d920f7f8f",
            },
            Witness {
                id: "dify-defaults",
                chart: "dify",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "ac28a34ae770d0ab",
            },
            Witness {
                id: "gitea-defaults",
                chart: "gitea",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "8f8a952b28b03e2e",
            },
            Witness {
                id: "kube-prometheus-stack-defaults",
                chart: "kube-prometheus-stack",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "dd9f4be83d0584aa",
            },
            Witness {
                id: "kyverno-defaults",
                chart: "kyverno",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "b08e545c10824d48",
            },
            Witness {
                id: "nats-defaults",
                chart: "nats",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "8d28717fa2b768cf",
            },
            Witness {
                id: "netbox-defaults",
                chart: "netbox",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "cb631ee69900f33b",
            },
            Witness {
                id: "okteto-defaults",
                chart: "okteto",
                kubernetes_version: "1.33.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "a4dc4c439352d91a",
            },
            Witness {
                id: "openebs-defaults",
                chart: "openebs",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "157d042bab5d5b28",
            },
            Witness {
                id: "prometheus-defaults",
                chart: "prometheus",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "124740a4fab72919",
            },
            Witness {
                id: "redmine-defaults",
                chart: "redmine",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "435dd318e69932c6",
            },
            Witness {
                id: "signoz-signoz-defaults",
                chart: "signoz-signoz",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "f91fb0853d332361",
            },
            Witness {
                id: "stacks-blockchain-api-defaults",
                chart: "stacks-blockchain-api",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "172a80c8d3c2932b",
            },
            Witness {
                id: "weblate-defaults",
                chart: "weblate",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "1c5e7befc673a62d",
            },
        ],
        size_obligations: &[],
        unfrozen: &[
            "airflow defaults: Helm renders without the schema and refuses the fixture's `/config/api/base_url` `pattern` under Go regexp; the jsonschema crate compiles that pattern, so the verdict is not reproducible offline.",
            "oncall defaults: Helm renders without the schema and refuses the shipped schema's `rabbitmq.ldap.uri` `pattern` under Go regexp; not reproducible offline.",
        ],
    },
];
