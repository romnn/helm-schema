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

/// Not a verdict: `chart`'s fixture, serialized as `output` writes it, must
/// fit Helm's file-size limit (`HELM_MAX_CHART_FILE_BYTES`, inclusive), or
/// Helm refuses the chart before any values are checked.
pub(crate) struct SizeObligation {
    pub(crate) id: &'static str,
    pub(crate) chart: &'static str,
    pub(crate) output: ShippedOutput,
    pub(crate) expectation: ObligationExpectation,
}

/// The serialization a size obligation measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShippedOutput {
    /// Compact JSON with short `$defs` keys: what `helm-schema lint` /
    /// `template` hand Helm.
    HelmReady,
    /// The default CLI file: pretty JSON with readable `$defs` keys when it fits
    /// Helm, otherwise compact JSON with short keys, including a final newline.
    DefaultCli,
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

const INT_OR_STRING_NULL_POLICY: &str = "the pinned strict v1.29.0 bundle types IntOrString as a `oneOf` that a null or empty targetPort matches twice, so it rejects the render exactly as the schema does, while the API server accepts an unset targetPort: is the pinned bundle the oracle for IntOrString nulls (F37/F39)?";

const RENDERS: OracleExpectation = OracleExpectation::Renders {
    kubernetes: KubernetesExpectation::NotRelevant,
};

pub(crate) const FAMILY_WITNESSES: &[FamilyWitnesses] = &[
    FamilyWitnesses {
        family: Family::F0,
        witnesses: &[
            Witness {
                id: "tigera-operator-kubernetes-provider-kind",
                chart: "tigera-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F0/tigera-operator-kubernetes-provider-kind.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "e97a05cab89aa859",
            },
            Witness {
                id: "tigera-operator-linux-dataplane-nftables",
                chart: "tigera-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F0/tigera-operator-linux-dataplane-nftables.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "16206054222a2c96",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
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
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
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
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "a0720c89b9f16e81",
            },
            Witness {
                id: "common-global-image-registry",
                chart: "common",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/common-global-image-registry.yaml"),
                oracle: RENDERS,
                // Not promoted: `common` is a library chart, so the standalone
                // render this row's RENDERS oracle names aborts ("library charts
                // are not installable"); the witness cannot express the umbrella
                // context Helm renders it in. The adopted fixture accepts, so the
                // pinned verdict moves; the policy question stays open for this row
                // until an umbrella witness is frozen and re-adjudicated.
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Accepts,
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
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "3b2430a5e8490fc1",
            },
            Witness {
                id: "harbor-global-image-registry",
                chart: "harbor",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/harbor-global-image-registry.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "32943d998bfbe134",
            },
            Witness {
                id: "karpenter-global-image-registry",
                chart: "karpenter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/karpenter-global-image-registry.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "2369164374809812",
            },
            Witness {
                id: "kubeview-global-image-registry",
                chart: "kubeview",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/kubeview-global-image-registry.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "2209cc3b511c5b9b",
            },
            Witness {
                id: "nginx-ingress-global-image-registry",
                chart: "nginx-ingress",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F1/nginx-ingress-global-image-registry.yaml"),
                oracle: RENDERS,
                // F1's `global` is admitted; the row still rejects through the
                // pre-existing D1 defaults defect `/properties/controller/allOf/10`
                // (the lost `hasKey .Values.controller.mgmt "usageReport"` guard,
                // plan/schema-bug-hunt-reports/report-batch-03.md:527), filed there.
                schema: SchemaExpectation::KnownFalseRejection,
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
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
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
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "b1ff9fbe794fa818",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F2,
        witnesses: &[
            Witness {
                id: "kyverno-mode-null",
                chart: "kyverno",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F2/kyverno-mode-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (kyverno/templates/validate.yaml:28:6): mode is not supported anymore, please remove it from your release and use admissionController.replicas instead.",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "3ab28f288921914f",
            },
            Witness {
                id: "kyverno-mode-string",
                chart: "kyverno",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F2/kyverno-mode-string.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (kyverno/templates/validate.yaml:28:6): mode is not supported anymore, please remove it from your release and use admissionController.replicas instead.",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "18f961ecd61223d7",
            },
            Witness {
                id: "bitnami-postgresql-resources-preset-huge",
                chart: "bitnami-postgresql",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F2/bitnami-postgresql-resources-preset-huge.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (postgresql/templates/primary/statefulset.yaml:483:25): ERROR: Preset key 'huge' invalid. Allowed values are nano,micro,small,medium,large,xlarge,2xlarge",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "b0cb14c01a95b58d",
            },
            Witness {
                id: "bitnami-postgresql-resources-preset-small",
                chart: "bitnami-postgresql",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F2/bitnami-postgresql-resources-preset-small.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "e49deaff79548ef8",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F3,
        witnesses: &[Witness {
            id: "rabbitmq-cluster-operator-fullname-override-null",
            chart: "rabbitmq-cluster-operator",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile(
                "F3/rabbitmq-cluster-operator-fullname-override-null.yaml",
            ),
            oracle: RENDERS,
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "0d2f3169288ebbb9",
        }],
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
                adjudicated: "2a9ba02632234c6c", // spellcheck:ignore-line
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
                adjudicated: "09b5212fb9ede737", // spellcheck:ignore-line
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
        family: Family::F7,
        witnesses: &[
            Witness {
                id: "eck-operator-metrics-secure-mode-enabled",
                chart: "eck-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F7/eck-operator-metrics-secure-mode-enabled.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (eck-operator/templates/statefulset.yaml:29:30): config.metrics.port must be greater than 0 when config.metrics.secureMode.enabled is true",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "bd460c2017e871cb",
            },
            Witness {
                id: "eck-operator-passwords-length-3",
                chart: "eck-operator",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F7/eck-operator-passwords-length-3.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (eck-operator/templates/statefulset.yaml:29:30): config.policies.passwords.length must be >= 6 and <= 72",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "63d3fcf506e1dbd0",
            },
            Witness {
                id: "prometheus-redis-exporter-service-monitor-targets-string-disabled",
                chart: "prometheus-redis-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F7/prometheus-redis-exporter-service-monitor-targets-string-disabled.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "a59b73f6f30eba4f",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F8,
        witnesses: &[
            Witness {
                id: "kafka-ui-ingress-host-null",
                chart: "kafka-ui",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F8/kafka-ui-ingress-host-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kafka-ui/templates/ingress.yaml:72:18 — wrong type for value; expected string; got interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "39f5d762a118c98a",
            },
            Witness {
                id: "kafka-ui-ingress-host-number",
                chart: "kafka-ui",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F8/kafka-ui-ingress-host-number.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kafka-ui/templates/ingress.yaml:72:18 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "5e1201dc0dd4f4a9",
            },
            Witness {
                id: "kafka-ui-ingress-preceding-paths-string",
                chart: "kafka-ui",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F8/kafka-ui-ingress-preceding-paths-string.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kafka-ui/templates/ingress.yaml:41:21 — can't evaluate field path in type interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "96445541bea3c27d",
            },
        ],
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
        family: Family::F10,
        witnesses: &[
            Witness {
                id: "qdrant-api-key-secret-key-null",
                chart: "qdrant",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F10/qdrant-api-key-secret-key-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: qdrant/templates/statefulset.yaml:31:28 — qdrant/templates/_helpers.tpl:89:31 — invalid value; expected string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "6be9b68fce3fdf20",
            },
            Witness {
                id: "qdrant-api-key-secret-key-string",
                chart: "qdrant",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F10/qdrant-api-key-secret-key-string.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "d685a53bb43349df",
            },
            Witness {
                id: "qdrant-read-only-api-key-secret-key-null",
                chart: "qdrant",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F10/qdrant-read-only-api-key-secret-key-null.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: qdrant/templates/statefulset.yaml:31:28 — qdrant/templates/_helpers.tpl:106:39 — invalid value; expected string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "7dfc1799509bb98d",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F11,
        witnesses: &[
            Witness {
                id: "qdrant-readiness-probe-null",
                chart: "qdrant",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F11/qdrant-readiness-probe-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: qdrant/templates/statefulset.yaml:139:28 — nil pointer evaluating interface {}.enabled",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "47ca7903c047eb03",
            },
            Witness {
                id: "qdrant-liveness-probe-null",
                chart: "qdrant",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F11/qdrant-liveness-probe-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: qdrant/templates/statefulset.yaml:119:28 — nil pointer evaluating interface {}.enabled",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "99b348b176335b1f",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F12,
        witnesses: &[Witness {
            id: "kibana-annotations-number-value",
            chart: "kibana",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F12/kibana-annotations-number-value.yaml"),
            oracle: RENDERS,
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "4106d1751ef83294",
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
        family: Family::F14,
        witnesses: &[
            Witness {
                id: "postgresql-ha-diagnostic-mode-enabled-null",
                chart: "postgresql-ha",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F14/postgresql-ha-diagnostic-mode-enabled-null.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: postgresql-ha/templates/postgresql/statefulset.yaml:185:89 — invalid value; expected bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "9b4e2bff7a1b3269",
            },
            Witness {
                id: "pgadmin4-image-registry-null",
                chart: "pgadmin4",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F14/pgadmin4-image-registry-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: pgadmin4/templates/deployment.yaml:89:20 — pgadmin4/templates/_helpers.tpl:68:92 — invalid value; expected string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "e2cb217faf7f483e",
            },
            Witness {
                id: "pgadmin4-image-registry-null-global-registry",
                chart: "pgadmin4",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F14/pgadmin4-image-registry-null-global-registry.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "5ddf23a6ac6e98cf",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F15,
        witnesses: &[
            Witness {
                id: "rabbitmq-memory-high-watermark-type-foo",
                chart: "rabbitmq",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F15/rabbitmq-memory-high-watermark-type-foo.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (rabbitmq/templates/validation.yaml:6:4): VALUES VALIDATION: rabbitmq: memoryHighWatermark.type",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "c5af181782a85a9a",
            },
            Witness {
                id: "rabbitmq-memory-high-watermark-type-absolute",
                chart: "rabbitmq",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F15/rabbitmq-memory-high-watermark-type-absolute.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "a4d61175911e59c6",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F16,
        witnesses: &[Witness {
            id: "vector-custom-config-values-example",
            chart: "vector",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F16/vector-custom-config-values-example.yaml"),
            oracle: OracleExpectation::Renders {
                kubernetes: KubernetesExpectation::Valid,
            },
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "ceb0d1a83873e7e0",
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
        family: Family::F18,
        witnesses: &[Witness {
            id: "synapse-macaroon-secret-key-null",
            chart: "synapse",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F18/synapse-macaroon-secret-key-null.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: synapse/templates/deployment.yaml:1:27 — wrong type for value; expected string; got interface {}",
            },
            // Masked: the fixture rejects this document only with the errors it already
            // gives the chart defaults (F4 `synapse-defaults`), so the verdict says nothing
            // about F18.
            schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
            adjudicated: "2181e36a8f27395d",
        }],
        size_obligations: &[],
        unfrozen: &[
            "synapse is the only chart calling `required <value> <message>`, and its fixture rejects its own defaults (F4), so `synapse-macaroon-secret-key-null` is rejected only through that unrelated rejection; re-adjudicate the row once the synapse defaults are accepted.",
        ],
    },
    FamilyWitnesses {
        family: Family::F19,
        witnesses: &[
            Witness {
                id: "nats-config-leafnodes-null",
                chart: "nats",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F19/nats-config-leafnodes-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: template: nats/templates/stateful-set.yaml:3:4 … gotpl:8:47: executing \"gotpl\" at <$configProtocol.enabled>: can't evaluate field enabled in type string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "df632397293ca7fc",
            },
            Witness {
                id: "nats-config-monitor-null",
                chart: "nats",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F19/nats-config-monitor-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: template: nats/templates/stateful-set.yaml:3:4 … gotpl:8:47: executing \"gotpl\" at <$configProtocol.enabled>: can't evaluate field enabled in type string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "57cc6076898993ad",
            },
            Witness {
                id: "nats-container-ports-nats-null",
                chart: "nats",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F19/nats-container-ports-nats-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: template: nats/templates/stateful-set.yaml:3:4 … gotpl:9:72: executing \"gotpl\" at <$containerPort>: wrong type for value; expected map[string]interface {}; got string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "d3c80034c4461046",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F20,
        witnesses: &[Witness {
            id: "cluster-autoscaler-pdb-max-unavailable-percent",
            chart: "cluster-autoscaler",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile(
                "F20/cluster-autoscaler-pdb-max-unavailable-percent.yaml",
            ),
            oracle: OracleExpectation::Renders {
                kubernetes: KubernetesExpectation::Valid,
            },
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "7cc0de498ec09c5b",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F21,
        witnesses: &[Witness {
            id: "minio-ingress-path-number-disabled",
            chart: "minio",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F21/minio-ingress-path-number-disabled.yaml"),
            oracle: RENDERS,
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "689d272e9f41eda5",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F22,
        witnesses: &[
            Witness {
                id: "clickhouse-liveness-probe-null",
                chart: "clickhouse",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F22/clickhouse-liveness-probe-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: clickhouse/templates/statefulset.yaml:201:23 — nil pointer evaluating interface {}.enabled",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "e824001734c35ffe",
            },
            Witness {
                id: "clickhouse-persistence-null",
                chart: "clickhouse",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F22/clickhouse-persistence-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: clickhouse/templates/statefulset.yaml:226:24 — nil pointer evaluating interface {}.volumeName",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "3193aa607740303b",
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
        family: Family::F24,
        witnesses: &[
            Witness {
                id: "cilium-flowlog-content-string-item",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F24/cilium-flowlog-content-string-item.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: cilium/templates/cilium-flowlog-configmap.yaml:19:20 — wrong type for value; expected map[string]interface {}; got string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "eaf5a292733e8a22",
            },
            Witness {
                id: "cilium-flowlog-content-string-item-no-file-size",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F24/cilium-flowlog-content-string-item-no-file-size.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "07a7c23665f9d783",
            },
            Witness {
                id: "clickhouse-readiness-probe-null",
                chart: "clickhouse",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F24/clickhouse-readiness-probe-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: clickhouse/templates/statefulset.yaml:208:23 — nil pointer evaluating interface {}.enabled",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "55a1bc2ec6e16d15",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F25,
        witnesses: &[
            Witness {
                id: "okteto-redis-image-null",
                chart: "okteto",
                kubernetes_version: "1.33.0",
                overrides: Overrides::ValuesFile("F25/okteto-redis-image-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (okteto/templates/redis-deployment.yaml:57:20): Invalid type for image value. Must be string or map.",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "cb2d799098776a5a",
            },
            Witness {
                id: "okteto-redis-image-string",
                chart: "okteto",
                kubernetes_version: "1.33.0",
                overrides: Overrides::ValuesFile("F25/okteto-redis-image-string.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "e7dd02e7f8042d0d",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F26,
        witnesses: &[
            Witness {
                id: "istiod-global-enable-tracing-true",
                chart: "istiod",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F26/istiod-global-enable-tracing-true.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "65a9f1761621ee4b",
            },
            Witness {
                id: "istiod-global-outbound-traffic-policy-map",
                chart: "istiod",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F26/istiod-global-outbound-traffic-policy-map.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "438b1c09c0388f15",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F27,
        witnesses: &[
            Witness {
                id: "base-profile-bogus-global-demo",
                chart: "base",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F27/base-profile-bogus-global-demo.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (base/templates/zzz_profile.yaml:31:3): unknown profile bogus",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "6d457224dc2365e0",
            },
            Witness {
                id: "base-profile-demo-global-bogus",
                chart: "base",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F27/base-profile-demo-global-bogus.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "40b18529d41b16ca",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F28,
        witnesses: &[
            Witness {
                id: "consul-datadog-otlp-protocol-bogus",
                chart: "consul",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F28/consul-datadog-otlp-protocol-bogus.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (consul/templates/_helpers.tpl:627:2): Valid values for global.metrics.datadog.otlp.protocol must be one of either \"http\" or \"grpc\".",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "5bcb8f8a71939484",
            },
            Witness {
                id: "consul-datadog-otlp-protocol-upper-grpc",
                chart: "consul",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F28/consul-datadog-otlp-protocol-upper-grpc.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "b6ab869a71e777c1",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F29,
        witnesses: &[
            Witness {
                id: "aws-node-termination-handler-probes-null",
                chart: "aws-node-termination-handler",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F29/aws-node-termination-handler-probes-null.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: aws-node-termination-handler/templates/daemonset.linux.yaml:77:31 — nil pointer evaluating interface {}.httpGet",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "2a9db035491b126f",
            },
            Witness {
                id: "minecraft-server-type-ftba-without-modpack",
                chart: "minecraft",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F29/minecraft-server-type-ftba-without-modpack.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (minecraft/templates/deployment.yaml:142:55): You must supply a minecraftserver.ftbModpackVersionID with type=FTBA",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "dfe476c2c7ef49ec",
            },
            Witness {
                id: "minecraft-restic-without-hostname",
                chart: "minecraft",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F29/minecraft-restic-without-hostname.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (minecraft/templates/deployment.yaml:392:56): mcbackup.resticHostname is required",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "8c64bd7169c04142",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
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
        family: Family::F32,
        witnesses: &[
            Witness {
                id: "falco-services-string-item",
                chart: "falco",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F32/falco-services-string-item.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: falco/templates/services.yaml:7:55 — can't evaluate field name in type interface {}",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "b199b568a3a91405",
            },
            Witness {
                id: "falco-services-named-item",
                chart: "falco",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F32/falco-services-named-item.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "61a8675ffa2a2358",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F33,
        witnesses: &[
            Witness {
                id: "falco-falco-plugins-null",
                chart: "falco",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F33/falco-falco-plugins-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: template: falco/templates/falcoctl-configmap.yaml:12:8: executing \"falco/templates/falcoctl-configmap.yaml\" at <include \"falco.containerPlugin\" .>: error calling include: template: falco/templates/_helpers.tpl:435:17: executing \"falco.containerPlugin\" at <append .Values.falco.plugins $pluginConfig>: error calling append: runtime error: invalid memory address or nil pointer dereference",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "a0a91f9cb8ff96fc",
            },
            Witness {
                id: "falco-falco-plugins-empty",
                chart: "falco",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F33/falco-falco-plugins-empty.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "273146e8747372d8",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F34,
        witnesses: &[
            Witness {
                id: "trino-worker-jvm-null",
                chart: "trino",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F34/trino-worker-jvm-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: trino/templates/deployment-worker.yaml:37:35 — trino/templates/configmap-worker.yaml:23:18 — nil pointer evaluating interface {}.maxHeapSize",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "731f61367292a748",
            },
            Witness {
                id: "trino-worker-liveness-probe-null",
                chart: "trino",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F34/trino-worker-liveness-probe-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: trino/templates/deployment-worker.yaml:201:43 — nil pointer evaluating interface {}.initialDelaySeconds",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "79ae7979799ef34a",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F35,
        witnesses: &[
            Witness {
                id: "trino-access-control-configmap",
                chart: "trino",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F35/trino-access-control-configmap.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "4eba7574ff63c335",
            },
            Witness {
                id: "trino-resource-groups-configmap-with-config",
                chart: "trino",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F35/trino-resource-groups-configmap-with-config.yaml",
                ),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Valid,
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "2201e4b71df0934b",
            },
            Witness {
                id: "trino-session-properties-configmap-with-config",
                chart: "trino",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F35/trino-session-properties-configmap-with-config.yaml",
                ),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Valid,
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "91e3953dc6b85e91",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F36,
        witnesses: &[
            Witness {
                id: "vault-standalone-config-null",
                chart: "vault",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F36/vault-standalone-config-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (vault/templates/_helpers.tpl:1168:4): structured server config is not supported, value must be a string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "2b252fed5a583607",
            },
            Witness {
                id: "vault-ha-config-null",
                chart: "vault",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F36/vault-ha-config-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (vault/templates/_helpers.tpl:1168:4): structured server config is not supported, value must be a string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "0ecd2c2da2e0fb3c",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F37,
        witnesses: &[
            Witness {
                id: "prometheus-pushgateway-service-target-port-null",
                chart: "prometheus-pushgateway",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F37/prometheus-pushgateway-service-target-port-null.yaml",
                ),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Invalid {
                        violations: &[ExpectedViolation {
                            kind: "Service",
                            instance_path: "/spec/ports/0/targetPort",
                            message: "'oneOf' failed, subschemas 0, 1 matched",
                        }],
                    },
                },
                // Helm renders `targetPort:` (null); the pinned bundle rejects it the way the
                // schema does.
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: INT_OR_STRING_NULL_POLICY,
                },
                adjudicated: "b125d6cf70441f30",
            },
            Witness {
                id: "alertmanager-defaults",
                chart: "alertmanager",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "bd00eb92672bffa2",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F38,
        witnesses: &[Witness {
            id: "nats-kafka-monitoring-http-port",
            chart: "nats-kafka",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F38/nats-kafka-monitoring-http-port.yaml"),
            oracle: OracleExpectation::Renders {
                kubernetes: KubernetesExpectation::Valid,
            },
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "30a3b4e39e5b5cf3",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F39,
        witnesses: &[
            Witness {
                id: "zabbix-postgresql-storage-size-empty",
                chart: "zabbix",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F39/zabbix-postgresql-storage-size-empty.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "f878b88ef999f22f",
            },
            Witness {
                id: "prometheus-pushgateway-service-target-port-empty",
                chart: "prometheus-pushgateway",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F39/prometheus-pushgateway-service-target-port-empty.yaml",
                ),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Invalid {
                        violations: &[ExpectedViolation {
                            kind: "Service",
                            instance_path: "/spec/ports/0/targetPort",
                            message: "'oneOf' failed, subschemas 0, 1 matched",
                        }],
                    },
                },
                schema: SchemaExpectation::PolicyUnresolved {
                    current: SchemaVerdict::Rejects,
                    question: INT_OR_STRING_NULL_POLICY,
                },
                adjudicated: "0e22d97c7d1e00f3",
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
        family: Family::F41,
        witnesses: &[
            Witness {
                id: "zabbix-postgres-access-secret-host-key-empty",
                chart: "zabbix",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F41/zabbix-postgres-access-secret-host-key-empty.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: YAML parse error on zabbix/templates/secret-db-access.yaml: error converting YAML to JSON: yaml: line 15: did not find expected key",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "19f381bb52feea8d",
            },
            Witness {
                id: "redis-ha-fullname-override-string",
                chart: "redis-ha",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F41/redis-ha-fullname-override-string.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "27512ed6c550baab",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F42,
        witnesses: &[
            Witness {
                id: "redis-ha-auth-true-without-password",
                chart: "redis-ha",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F42/redis-ha-auth-true-without-password.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: redis-ha/templates/redis-auth-secret.yaml:18:52 — invalid value; expected string",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "77d30373bf7c7337",
            },
            Witness {
                id: "redis-ha-auth-true-with-password",
                chart: "redis-ha",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F42/redis-ha-auth-true-with-password.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "e6bbdccb391c5661",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F43,
        witnesses: &[Witness {
            id: "zabbix-server-host-port-string-true",
            chart: "zabbix",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F43/zabbix-server-host-port-string-true.yaml"),
            oracle: RENDERS,
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "732a8769bb3a51b2",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F44,
        witnesses: &[Witness {
            id: "loki-use-test-schema",
            chart: "loki",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F44/loki-use-test-schema.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: execution error at (loki/templates/write/statefulset-write.yaml:50:28): Please define loki.storage.bucketNames.chunks",
            },
            schema: SchemaExpectation::KnownFalseAcceptance,
            adjudicated: "fd15ee240f877c29",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F45,
        witnesses: &[
            Witness {
                id: "x509-certificate-exporter-disable-builtin-without-extra-groups",
                chart: "x509-certificate-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F45/x509-certificate-exporter-disable-builtin-without-extra-groups.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (x509-certificate-exporter/templates/prometheusrule.yaml:28:9): Extra alert groups (extraAlertGroups) are required when disableBuiltinAlertGroup is set!",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "432a4f4aac94f26a",
            },
            Witness {
                id: "x509-certificate-exporter-disable-builtin-with-extra-groups",
                chart: "x509-certificate-exporter",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F45/x509-certificate-exporter-disable-builtin-with-extra-groups.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "47b1b8d942518508",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F46,
        witnesses: &[Witness {
            id: "datadog-operator-migration-preview",
            chart: "datadog",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F46/datadog-operator-migration-preview.yaml"),
            oracle: RENDERS,
            schema: SchemaExpectation::KnownFalseRejection,
            adjudicated: "7af29168d688a7b1",
        }],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F47,
        witnesses: &[
            Witness {
                id: "jupyterhub-rbac-enabled",
                chart: "jupyterhub",
                kubernetes_version: "1.33.0",
                overrides: Overrides::ValuesFile("F47/jupyterhub-rbac-enabled.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (jupyterhub/templates/NOTES.txt:175:4): ################################################################################# ######   BREAKING: The config values passed contained no longer accepted    #####",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "c907d960aba7b150",
            },
            Witness {
                id: "jupyterhub-hub-fs-gid",
                chart: "jupyterhub",
                kubernetes_version: "1.33.0",
                overrides: Overrides::ValuesFile("F47/jupyterhub-hub-fs-gid.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (jupyterhub/templates/NOTES.txt:175:4): ################################################################################# ######   BREAKING: The config values passed contained no longer accepted    #####",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "a9ae7732f55b1acc",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F48,
        witnesses: &[
            Witness {
                id: "logstash-secrets-value-number",
                chart: "logstash",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F48/logstash-secrets-value-number.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: logstash/templates/statefulset.yaml:62:28 — logstash/templates/secret.yaml:22:24 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "9b8d00e3e3e175e1",
            },
            Witness {
                id: "logstash-secrets-value-string",
                chart: "logstash",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F48/logstash-secrets-value-string.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "db9c2f5846ec56d0",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F49,
        witnesses: &[
            Witness {
                id: "imgproxy-defaults",
                chart: "imgproxy",
                kubernetes_version: "1.29.0",
                overrides: Overrides::Set(&[]),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "d5f311498add0be1",
            },
            Witness {
                id: "nginx-ingress-controller-tcp-number",
                chart: "nginx-ingress-controller",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F49/nginx-ingress-controller-tcp-number.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: nginx-ingress-controller/templates/controller-networkpolicy.yaml:39:41 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "7dcb240a8f4f76bd",
            },
            Witness {
                id: "nginx-ingress-controller-tcp-string",
                chart: "nginx-ingress-controller",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F49/nginx-ingress-controller-tcp-string.yaml"),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Valid,
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "c992c3eb40a5f779",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F50,
        witnesses: &[
            Witness {
                id: "eck-stack-kibana-spec-ref-namespace-only",
                chart: "eck-stack",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F50/eck-stack-kibana-spec-ref-namespace-only.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (eck-stack/charts/eck-kibana/templates/kibana.yaml:27:5): An elasticsearchRef name or secretName is required",
                },
                // Masked: the fixture rejects this document only with the errors it already
                // gives the chart defaults (F51 `eck-stack-defaults`, `/properties/eck-
                // kibana/allOf/16`), so the verdict says nothing about F50.
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "33e684c276f5bf4b",
            },
            Witness {
                id: "eck-stack-kibana-spec-ref-name-fallback-null",
                chart: "eck-stack",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F50/eck-stack-kibana-spec-ref-name-fallback-null.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "f6af6dc42884e662",
            },
        ],
        size_obligations: &[],
        unfrozen: &[
            "`eck-stack-kibana-spec-ref-namespace-only` is rejected only by the arm that also rejects the eck-stack defaults (F51); re-adjudicate it once the defaults are accepted.",
        ],
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
        family: Family::F52,
        witnesses: &[
            Witness {
                id: "kubeshark-support-chat-enabled-null",
                chart: "kubeshark",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F52/kubeshark-support-chat-enabled-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kubeshark/templates/06-front-deployment.yaml:82:102 — invalid value; expected bool",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "44a17068b049905b",
            },
            Witness {
                id: "kubeshark-support-chat-enabled-false",
                chart: "kubeshark",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F52/kubeshark-support-chat-enabled-false.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "a67da0ca1bbb13cb",
            },
        ],
        size_obligations: &[],
        unfrozen: &[
            "open-webui `persistence.enabled: null` (workload-manager.yaml:6, the plan's witness): open-webui declares `import-values`, which the gate's coalescer does not model, so the chart cannot be enrolled.",
        ],
    },
    FamilyWitnesses {
        family: Family::F53,
        witnesses: &[
            Witness {
                id: "argocd-apps-applicationset-template-patch-map",
                chart: "argocd-apps",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F53/argocd-apps-applicationset-template-patch-map.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: argocd-apps/templates/applicationsets.yaml:103:20 — wrong type for value; expected string; got map[string]interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "876e0e0a87bcf973",
            },
            Witness {
                id: "argocd-apps-applicationset-template-patch-string",
                chart: "argocd-apps",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F53/argocd-apps-applicationset-template-patch-string.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "d8a7b96e385ac21c",
            },
            Witness {
                id: "argocd-apps-application-project-number",
                chart: "argocd-apps",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F53/argocd-apps-application-project-number.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: argocd-apps/templates/applications.yaml:28:26 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "1103aa1f361f9c20",
            },
        ],
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
                adjudicated: "30089c54688ba952", // spellcheck:ignore-line
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
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
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
        family: Family::F55,
        witnesses: &[
            Witness {
                id: "alloy-alloy-mounts-null",
                chart: "alloy",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F55/alloy-alloy-mounts-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: alloy/templates/controllers/daemonset.yaml:27:8 — alloy/templates/containers/_agent.yaml:93:18 — nil pointer evaluating interface {}.varlog",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "1dbbb2392e50fb6c",
            },
            Witness {
                id: "alloy-alloy-clustering-null",
                chart: "alloy",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F55/alloy-alloy-clustering-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: alloy/templates/controllers/daemonset.yaml:27:8 — alloy/templates/containers/_agent.yaml:19:18 — nil pointer evaluating interface {}.enabled",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "bfb3e5574cdd0565",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F56,
        witnesses: &[
            Witness {
                id: "alloy-hpa-config-reloader-requests-null",
                chart: "alloy",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F56/alloy-hpa-config-reloader-requests-null.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (alloy/templates/hpa.yaml:10:57): .Values.configReloader.resources.requests is required when using autoscaling.",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "b2fdfab3b6538584",
            },
            Witness {
                id: "alloy-hpa-requests-set",
                chart: "alloy",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F56/alloy-hpa-requests-set.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "0d22cca203985626",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F57,
        witnesses: &[
            Witness {
                id: "jenkins-config-auto-reload-image-null",
                chart: "jenkins",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F57/jenkins-config-auto-reload-image-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: jenkins/templates/jenkins-controller-statefulset.yaml:122:4 — jenkins/templates/_helpers.tpl:639:18 — nil pointer evaluating interface {}.registry",
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "cfac41d1be2c4072",
            },
            Witness {
                id: "jenkins-config-auto-reload-disabled-image-null",
                chart: "jenkins",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F57/jenkins-config-auto-reload-disabled-image-null.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "b9d8d8c2b16ea506",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F58,
        witnesses: &[
            Witness {
                id: "kube-prometheus-stack-alertmanager-ingress-paths-number",
                chart: "kube-prometheus-stack",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F58/kube-prometheus-stack-alertmanager-ingress-paths-number.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: kube-prometheus-stack/templates/alertmanager/ingress.yaml:38:25 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "4cfb21450e3a69b0",
            },
            Witness {
                id: "kube-prometheus-stack-alertmanager-ingress-paths-string",
                chart: "kube-prometheus-stack",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F58/kube-prometheus-stack-alertmanager-ingress-paths-string.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "a0d4b9b2f8b1b373",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F59,
        witnesses: &[
            Witness {
                id: "kube-prometheus-stack-prometheus-network-policy-flavor-list-disabled",
                chart: "kube-prometheus-stack",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F59/kube-prometheus-stack-prometheus-network-policy-flavor-list-disabled.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "39eefb18079e995f",
            },
            Witness {
                id: "kube-prometheus-stack-operator-network-policy-flavor-list-disabled",
                chart: "kube-prometheus-stack",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F59/kube-prometheus-stack-operator-network-policy-flavor-list-disabled.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "65770abde7af185f",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F60,
        witnesses: &[
            Witness {
                id: "vault-extra-volumes-secret",
                chart: "vault",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F60/vault-extra-volumes-secret.yaml"),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Valid,
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "879320de27a67e48",
            },
            Witness {
                id: "vault-extra-volumes-config-map",
                chart: "vault",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F60/vault-extra-volumes-config-map.yaml"),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Valid,
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "5649ab1b40ded0a2", // spellcheck:ignore-line
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F61,
        witnesses: &[
            Witness {
                id: "datadog-cluster-checks-share-process-namespace-string",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F61/datadog-cluster-checks-share-process-namespace-string.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "027c7c6208ef4a57",
            },
            Witness {
                id: "datadog-agents-share-process-namespace-maybe",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F61/datadog-agents-share-process-namespace-maybe.yaml",
                ),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Invalid {
                        violations: &[ExpectedViolation {
                            kind: "DaemonSet",
                            instance_path: "/spec/template/spec/shareProcessNamespace",
                            message: "got string, want null or boolean",
                        }],
                    },
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "6a8840d4050f5ab8",
            },
            Witness {
                id: "datadog-cluster-agent-share-process-namespace-maybe",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F61/datadog-cluster-agent-share-process-namespace-maybe.yaml",
                ),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Invalid {
                        violations: &[ExpectedViolation {
                            kind: "Deployment",
                            instance_path: "/spec/template/spec/shareProcessNamespace",
                            message: "got string, want null or boolean",
                        }],
                    },
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "27b172dc62ca4f12",
            },
            Witness {
                id: "datadog-cluster-checks-runner-share-process-namespace-maybe",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F61/datadog-cluster-checks-runner-share-process-namespace-maybe.yaml",
                ),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Invalid {
                        violations: &[ExpectedViolation {
                            kind: "Deployment",
                            instance_path: "/spec/template/spec/shareProcessNamespace",
                            message: "got string, want null or boolean",
                        }],
                    },
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "a9765a77d94202b6",
            },
            Witness {
                id: "datadog-otel-agent-gateway-share-process-namespace-maybe",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F61/datadog-otel-agent-gateway-share-process-namespace-maybe.yaml",
                ),
                oracle: OracleExpectation::Renders {
                    kubernetes: KubernetesExpectation::Invalid {
                        violations: &[ExpectedViolation {
                            kind: "Deployment",
                            instance_path: "/spec/template/spec/shareProcessNamespace",
                            message: "got string, want null or boolean",
                        }],
                    },
                },
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "41e16888309e5c97",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F62,
        witnesses: &[
            Witness {
                id: "weblate-postgresql-ldap-null",
                chart: "weblate",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F62/weblate-postgresql-ldap-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: weblate/charts/postgresql/templates/primary/statefulset.yaml:340:50 — nil pointer evaluating interface {}.enabled",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "f22fb4047c273dd6",
            },
            Witness {
                id: "weblate-postgresql-audit-null",
                chart: "weblate",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F62/weblate-postgresql-audit-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: weblate/charts/postgresql/templates/primary/statefulset.yaml:398:31 — nil pointer evaluating interface {}.logHostname",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "2ad4b6c4501345ae",
            },
        ],
        size_obligations: &[],
        unfrozen: &[
            "airflow `postgresql.*` nil dereferences (34 aborts) and the oncall subchart witnesses: Helm refuses both shipped fixtures under Go regexp (`/config/api/base_url`, `rabbitmq.ldap.uri`), so the verdict is not reproducible offline; the milvus witnesses are masked by its D3 defaults rejection.",
        ],
    },
    FamilyWitnesses {
        family: Family::F63,
        witnesses: &[Witness {
            id: "milvus-ingress-tls-string-item",
            chart: "milvus",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F63/milvus-ingress-tls-string-item.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: milvus/templates/ingress.yaml:71:16 — can't evaluate field hosts in type interface {}",
            },
            // Masked: the fixture rejects this document only with the errors it already
            // gives the chart defaults (D3 `milvus-defaults`), so the verdict says nothing
            // about F63.
            schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
            adjudicated: "24d12d1043dfa03e",
        }],
        size_obligations: &[],
        unfrozen: &[
            "milvus `ingress.tls: [\"a\"]` is rejected only by the D3 arms that reject the milvus defaults, and oncall's fixture is refused by Helm under Go regexp; no unmasked witness exists in the corpus.",
        ],
    },
    FamilyWitnesses {
        family: Family::F64,
        witnesses: &[],
        size_obligations: &[],
        unfrozen: &[
            "oncall `oncall.smtp.ssl: true` (SMTP tls+ssl guard): Helm refuses oncall's shipped fixture under Go regexp (`rabbitmq.ldap.uri`), so the verdict is not reproducible offline; no other chart has the shape.",
        ],
    },
    FamilyWitnesses {
        family: Family::F65,
        witnesses: &[Witness {
            id: "gitea-replica-count-2",
            chart: "gitea",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F65/gitea-replica-count-2.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: execution error at (gitea/templates/gitea/deployment.yaml:30:28): When using multiple replicas, a RWX file system is required and persistence.accessModes[0] must be set to ReadWriteMany.",
            },
            // Masked: the fixture rejects this document only with the errors it already
            // gives the chart defaults (`gitea-defaults`), so the verdict says nothing
            // about F65.
            schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
            adjudicated: "87086a69c5606a55",
        }],
        size_obligations: &[],
        unfrozen: &[
            "`gitea-replica-count-2` is rejected only by the errors the gitea defaults already have; re-adjudicate once `gitea-defaults` is accepted.",
        ],
    },
    FamilyWitnesses {
        family: Family::F66,
        witnesses: &[Witness {
            id: "gitea-client-settings-policy-empty-body",
            chart: "gitea",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F66/gitea-client-settings-policy-empty-body.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: execution error at (gitea/templates/gitea/clientSettingsPolicy.yaml:3:4): gatewayAPI.nginx.clientSettingsPolicies.body is required",
            },
            // Masked: the fixture rejects this document only with the errors it already
            // gives the chart defaults (`gitea-defaults`), so the verdict says nothing
            // about F66.
            schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
            adjudicated: "4eb16220b9c24c9d",
        }],
        size_obligations: &[],
        unfrozen: &[
            "`gitea-client-settings-policy-empty-body` is rejected only by the errors the gitea defaults already have; re-adjudicate once `gitea-defaults` is accepted.",
        ],
    },
    FamilyWitnesses {
        family: Family::F67,
        witnesses: &[Witness {
            id: "gitea-pod-security-context-string",
            chart: "gitea",
            kubernetes_version: "1.29.0",
            overrides: Overrides::ValuesFile("F67/gitea-pod-security-context-string.yaml"),
            oracle: OracleExpectation::Aborts {
                diagnostic: "Error: gitea/templates/gitea/deployment.yaml:47:33 — gitea/templates/_helpers.tpl:109:81 — wrong type for value; expected map[string]interface {}; got string",
            },
            // Masked: the fixture rejects this document only with the errors it already
            // gives the chart defaults (`gitea-defaults`), so the verdict says nothing
            // about F67.
            schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
            adjudicated: "72434ea88a1f7054",
        }],
        size_obligations: &[],
        unfrozen: &[
            "`gitea-pod-security-context-string` is rejected only by the errors the gitea defaults already have; re-adjudicate once `gitea-defaults` is accepted.",
        ],
    },
    FamilyWitnesses {
        family: Family::F68,
        witnesses: &[
            Witness {
                id: "datadog-dogstatsd-socket-path-number",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F68/datadog-dogstatsd-socket-path-number.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/templates/NOTES.txt:672:29 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "1507c3857372f7d9",
            },
            Witness {
                id: "datadog-apm-socket-path-number",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F68/datadog-apm-socket-path-number.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: datadog/templates/NOTES.txt:672:72 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "e049391b08b6caf2", // spellcheck:ignore-line
            },
            Witness {
                id: "synapse-form-secret-number",
                chart: "synapse",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F68/synapse-form-secret-number.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: synapse/templates/deployment.yaml:4:27 — wrong type for value; expected string; got float64",
                },
                // The one error beyond the 25 F4 defaults errors is the declared `homeserver`
                // string typing, not the `required` string-parameter contract.
                schema: SchemaExpectation::Fixed(SchemaVerdict::Rejects),
                adjudicated: "6edce1291d2415c9",
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
                adjudicated: "1cf5840736cb8ba8", // spellcheck:ignore-line
            },
            Witness {
                id: "cilium-clusters-empty-list",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F69/cilium-clusters-empty-list.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "932c2cb785661ca5",
            },
            Witness {
                id: "cilium-clusters-empty-map",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F69/cilium-clusters-empty-map.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "af54049216a4bceb",
            },
            Witness {
                id: "cilium-clustermesh-enabled",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F69/cilium-clustermesh-enabled.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "664ee9c5f6284236",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F70,
        witnesses: &[
            Witness {
                id: "cilium-eni-iam-role-operator-annotations-null",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F70/cilium-eni-iam-role-operator-annotations-null.yaml",
                ),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: cilium/templates/cilium-operator/serviceaccount.yaml:3:22 — wrong type for value; expected map[string]interface {}; got interface {}",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "f16a8480a8f92936",
            },
            Witness {
                id: "cilium-tls-ca-cert-key-numbers",
                chart: "cilium",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F70/cilium-tls-ca-cert-key-numbers.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: cilium/templates/hubble/tls-helm/server-secret.yaml:2:10 — cilium/templates/_helpers.tpl:77:32 — wrong type for value; expected string; got float64",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "a81484db0f62bcd2",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F71,
        witnesses: &[
            Witness {
                id: "traefik-image-tag-below-proxy-min",
                chart: "traefik",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F71/traefik-image-tag-below-proxy-min.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (traefik/templates/requirements.yaml:6:8): ERROR: This version of the Chart only supports Traefik Proxy v3.6.0+",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "729a21a9a019fd1d",
            },
            Witness {
                id: "datadog-agents-image-tag-below-floor",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F71/datadog-agents-image-tag-below-floor.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (datadog/templates/_helpers.tpl:116:4): This version of the chart requires an agent image 7.36.0 or greater. If you want to force and skip this check, use `--set agents.image.doNotCheckTag=true`",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "efe6825441aead9f",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F72,
        witnesses: &[
            Witness {
                id: "traefik-liveness-probe-null",
                chart: "traefik",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F72/traefik-liveness-probe-null.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "ab9d3d32490202b4",
            },
            Witness {
                id: "traefik-readiness-probe-null",
                chart: "traefik",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F72/traefik-readiness-probe-null.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "4b12b4e9a039af59",
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
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "datadog-fixture-size",
                chart: "datadog",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "dify-fixture-size",
                chart: "dify",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "gitea-fixture-size",
                chart: "gitea",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "kube-prometheus-stack-fixture-size",
                chart: "kube-prometheus-stack",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "kyverno-fixture-size",
                chart: "kyverno",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "milvus-fixture-size",
                chart: "milvus",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "nats-fixture-size",
                chart: "nats",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "netbox-fixture-size",
                chart: "netbox",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "okteto-fixture-size",
                chart: "okteto",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "oncall-fixture-size",
                chart: "oncall",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "openebs-fixture-size",
                chart: "openebs",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "prometheus-fixture-size",
                chart: "prometheus",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "redmine-fixture-size",
                chart: "redmine",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "signoz-signoz-fixture-size",
                chart: "signoz-signoz",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "stacks-blockchain-api-fixture-size",
                chart: "stacks-blockchain-api",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "synapse-fixture-size",
                chart: "synapse",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "weblate-fixture-size",
                chart: "weblate",
                output: ShippedOutput::HelmReady,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "airflow-default-output-size",
                chart: "airflow",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "datadog-default-output-size",
                chart: "datadog",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "dify-default-output-size",
                chart: "dify",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "gitea-default-output-size",
                chart: "gitea",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "kube-prometheus-stack-default-output-size",
                chart: "kube-prometheus-stack",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "kyverno-default-output-size",
                chart: "kyverno",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "milvus-default-output-size",
                chart: "milvus",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "nats-default-output-size",
                chart: "nats",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "netbox-default-output-size",
                chart: "netbox",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "okteto-default-output-size",
                chart: "okteto",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "oncall-default-output-size",
                chart: "oncall",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "openebs-default-output-size",
                chart: "openebs",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "prometheus-default-output-size",
                chart: "prometheus",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "redmine-default-output-size",
                chart: "redmine",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "signoz-signoz-default-output-size",
                chart: "signoz-signoz",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "stacks-blockchain-api-default-output-size",
                chart: "stacks-blockchain-api",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "synapse-default-output-size",
                chart: "synapse",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
            SizeObligation {
                id: "weblate-default-output-size",
                chart: "weblate",
                output: ShippedOutput::DefaultCli,
                expectation: ObligationExpectation::Met,
            },
        ],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F75,
        witnesses: &[
            Witness {
                id: "datadog-cluster-name-90-lowercase",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F75/datadog-cluster-name-90-lowercase.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: execution error at (datadog/templates/cluster-agent-deployment.yaml:188:14): Your `clusterName` isn't valid, it must be 80 characters or less.",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "4c57b55e223890ab",
            },
            Witness {
                id: "datadog-cluster-name-80-lowercase",
                chart: "datadog",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F75/datadog-cluster-name-80-lowercase.yaml"),
                oracle: RENDERS,
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
                adjudicated: "037a8a5c5321bb6d",
            },
        ],
        size_obligations: &[],
        unfrozen: &[],
    },
    FamilyWitnesses {
        family: Family::F76,
        witnesses: &[
            Witness {
                id: "surveyor-image-repository-null-registry",
                chart: "surveyor",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "F76/surveyor-image-repository-null-registry.yaml",
                ),
                oracle: RENDERS,
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "80884189f380e2e6",
            },
            Witness {
                id: "surveyor-image-repository-null",
                chart: "surveyor",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("F76/surveyor-image-repository-null.yaml"),
                oracle: OracleExpectation::Aborts {
                    diagnostic: "Error: YAML parse error on surveyor/templates/deployment.yaml: error converting YAML to JSON: yaml: line 33: found character that cannot start any token",
                },
                schema: SchemaExpectation::KnownFalseAcceptance,
                adjudicated: "b87bcd85e87728dc",
            },
        ],
        size_obligations: &[],
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
        family: Family::D4,
        witnesses: &[
            Witness {
                id: "stacks-blockchain-api-postgresql-auth-username",
                chart: "stacks-blockchain-api",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile(
                    "D4/stacks-blockchain-api-postgresql-auth-username.yaml",
                ),
                oracle: RENDERS,
                // Rejected by the ten parent-root arms `stacks-blockchain-api-defaults` also
                // hits.
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "ac63ffaef3a21bc2",
            },
            Witness {
                id: "netbox-postgresql-auth-username",
                chart: "netbox",
                kubernetes_version: "1.29.0",
                overrides: Overrides::ValuesFile("D4/netbox-postgresql-auth-username.yaml"),
                oracle: RENDERS,
                // Rejected by the five root arms `netbox-defaults` also hits (D3 and D4
                // together).
                schema: SchemaExpectation::KnownFalseRejection,
                adjudicated: "9741e34664f7db1c",
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
                adjudicated: "88deed4025ba59b9", // spellcheck:ignore-line
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
                schema: SchemaExpectation::Fixed(SchemaVerdict::Accepts),
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
