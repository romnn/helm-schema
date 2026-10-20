//! Exact hard failures inherited by the battery, with no allowance for missing entries.

use color_eyre::eyre;
use serde::Serialize;

use super::HelmAdjudicationCoverage;
use super::known_false_acceptances::{
    Baseline, ContextSelector, ContextTransport, Probe, Rejection,
};

pub(crate) enum FailureSelector {
    Context(ContextSelector),
    Probe {
        probe: Probe,
        kubernetes_version: &'static str,
    },
}

impl FailureSelector {
    fn case(&self, chart: &str) -> eyre::Result<String> {
        match self {
            Self::Context(selector) => selector.case(chart),
            Self::Probe { probe, .. } => Ok(format!("{chart}: {} <- {}", probe.path, probe.value)),
        }
    }

    fn kubernetes_version(&self) -> &str {
        match self {
            Self::Context(selector) => selector.kubernetes_version,
            Self::Probe {
                kubernetes_version, ..
            } => kubernetes_version,
        }
    }
}

pub(crate) enum FailureClass {
    Adjudication {
        reason: &'static str,
    },
    OracleFalseAcceptance {
        rejection: Rejection,
        baseline: Baseline,
        kubernetes_errors: &'static [&'static str],
    },
}

pub(crate) struct KnownInheritedFailure {
    pub(crate) chart: &'static str,
    pub(crate) selector: FailureSelector,
    pub(crate) failure: FailureClass,
    pub(crate) owner: &'static str,
    pub(crate) observed_on: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct ObservedFailure {
    pub(crate) case: String,
    pub(crate) kubernetes_version: String,
    pub(crate) reason: String,
    pub(crate) evidence: Option<std::path::PathBuf>,
}

impl ObservedFailure {
    pub(crate) fn from_adjudication(
        case: String,
        kubernetes_version: String,
        message: &str,
    ) -> Self {
        let reason = message
            .strip_prefix(&format!("{case}: "))
            .unwrap_or(message);
        let (reason, evidence) = match reason.rsplit_once("; evidence=") {
            Some((reason, path)) => (reason, Some(std::path::PathBuf::from(path))),
            None => (reason, None),
        };
        let reason = case.split_once(": ").map_or(reason, |(_, probe)| {
            reason
                .strip_suffix(&format!("; case={probe}"))
                .unwrap_or(reason)
        });
        Self {
            case,
            kubernetes_version,
            reason: reason.to_string(),
            evidence,
        }
    }
}

impl KnownInheritedFailure {
    pub(crate) fn lists(&self, observed: &ObservedFailure) -> eyre::Result<bool> {
        let FailureClass::Adjudication { reason } = self.failure else {
            return Ok(false);
        };
        Ok(self.selector.case(self.chart)? == observed.case
            && self.selector.kubernetes_version() == observed.kubernetes_version
            && reason == observed.reason)
    }
}

pub(crate) fn oracle_false_acceptance_is_listed(
    rows: &[KnownInheritedFailure],
    observed: &super::ObservedFalseAcceptance,
) -> eyre::Result<bool> {
    for row in rows {
        if row.lists_oracle_false_acceptance(observed)? {
            return Ok(true);
        }
    }
    Ok(false)
}

impl KnownInheritedFailure {
    fn lists_oracle_false_acceptance(
        &self,
        observed: &super::ObservedFalseAcceptance,
    ) -> eyre::Result<bool> {
        let FailureClass::OracleFalseAcceptance {
            rejection,
            baseline,
            kubernetes_errors,
        } = &self.failure
        else {
            return Ok(false);
        };
        if self.selector.case(self.chart)? != observed.case
            || *rejection != observed.rejection
            || *baseline != observed.baseline
        {
            return Ok(false);
        }
        let Some(evidence) = &observed.evidence else {
            return Ok(false);
        };
        let evidence: serde_json::Value =
            serde_json::from_slice(&std::fs::read(evidence.join("schema-verdicts.json"))?)?;
        Ok(
            evidence.get("new_kubernetes_errors") == Some(&serde_json::json!(kubernetes_errors))
                && evidence.get("kubernetes_version")
                    == Some(&serde_json::json!(self.selector.kubernetes_version()))
                && evidence.get("transport") == Some(&serde_json::json!("values-file")),
        )
    }
}

/// Every observed hard failure must be listed, and every entry must recur.
/// A filtered diagnostic run cannot certify the complete inherited list.
pub(crate) fn validate_inherited_failures(
    coverage: &HelmAdjudicationCoverage,
    rows: &[KnownInheritedFailure],
) -> eyre::Result<usize> {
    let mut problems = Vec::new();
    let mut identities = std::collections::BTreeSet::new();
    for row in rows {
        let case = row.selector.case(row.chart)?;
        eyre::ensure!(
            identities.insert(case.clone()),
            "duplicate inherited failure: {case}"
        );
        eyre::ensure!(
            !row.owner.is_empty() && !row.observed_on.is_empty(),
            "inherited failure has no owner or observation: {case}"
        );
        let mut observed = false;
        match row.failure {
            FailureClass::Adjudication { .. } => {
                for failure in &coverage.adjudication_failures {
                    observed |= row.lists(failure)?;
                }
            }
            FailureClass::OracleFalseAcceptance { .. } => {
                for failure in &coverage.false_acceptances {
                    observed |= row.lists_oracle_false_acceptance(failure)?;
                }
            }
        }
        if observed {
            eprintln!(
                "INHERITED_HARD_FAILURE owner={} observed_on={}: {case}",
                row.owner, row.observed_on
            );
        } else {
            problems.push(format!(
                "stale inherited failure; remove it: {case} (owner={}, observed_on={})",
                row.owner, row.observed_on
            ));
        }
    }
    for observed in &coverage.adjudication_failures {
        let mut listed = false;
        for row in rows {
            listed |= row.lists(observed)?;
        }
        if !listed {
            problems.push(format!("unlisted hard failure: {observed:?}"));
        }
    }
    let mut oracle_false_acceptances = 0;
    for observed in &coverage.false_acceptances {
        if oracle_false_acceptance_is_listed(rows, observed)? {
            oracle_false_acceptances += 1;
        }
    }
    eyre::ensure!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(oracle_false_acceptances)
}

pub(crate) const KNOWN_INHERITED_FAILURES: &[KnownInheritedFailure] = &[
    KnownInheritedFailure {
        chart: "datadog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"datadog":{"operator":{"enabled":true}},"operator":{"installCRDs":null},"tags":{"install-crds":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "datadog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"datadog":{"operator":{"enabled":true}},"operator":{"installCRDs":null},"tags":{"install-crds":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "goldilocks",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"vpa":{"enabled":true,"metrics-server":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "goldilocks",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"vpa":{"enabled":true,"metrics-server":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "goldilocks",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"vpa":{"enabled":true,"metrics-server":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"community-operator":null,"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"community-operator":{"community-operator-crds":null},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":null}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":true}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"community-operator":null,"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"community-operator":{"community-operator-crds":null},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":null}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":true}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"community-operator":null,"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"community-operator":{"community-operator-crds":null},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "graylog",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"community-operator":{"community-operator-crds":{"enabled":true}},"tags":{"install-mongodb":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 8: apiextensions.k8s.io/v1/CustomResourceDefinition mongodbcommunity.mongodbcommunity.mongodb.com: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"temporal":{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"elasticsearch":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "lean/schema-emission-temporal-wrapper",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"temporal":{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "metallb",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"frr-k8s":{"crds":{"enabled":null}},"frrk8s":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "metallb",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"frr-k8s":{"crds":{"enabled":null}},"frrk8s":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "metallb",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"frr-k8s":{"crds":{"enabled":null}},"frrk8s":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "nacos",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mysql":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"cainjector":{"containerSecurityContext":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"containerSecurityContext":{"allowPrivilegeEscalation":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"global":{"podSecurityPolicy":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"prometheus":{"servicemonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"prometheus":{"servicemonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"startupapicheck":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"webhook":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true,"webhook":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"cert-manager":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"autoscaling":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"containerSecurityContext":{"allowPrivilegeEscalation":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"downloadDashboards":{"securityContext":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enableKubeBackwardCompatibility":true,"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"headlessService":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"autoscaling":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"networkPolicy":{"limitEgress":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"ingress":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"initChownData":{"securityContext":{"runAsNonRoot":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"egress":{"blockDNSResolution":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"egress":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"persistence":{"disableWarning":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"persistence":{"inMemory":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"rbac":{"namespaced":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"rbac":{"pspUseAppArmor":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"serviceAccount":{"automountServiceAccountToken":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"serviceMonitor":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"initAlerts":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"dashboards":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"dashboards":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"initDatasources":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"enableUniqueFilenames":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"initNotifiers":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"initPlugins":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"securityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true,"useStatefulSet":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"grafana":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"autoscaling":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"hostNetwork":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"hostPort":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"image":{"chroot":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"ingressClassByName":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"ingressClassResource":{"default":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"keda":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"keda":{"restoreToOriginalReplicaCount":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"metrics":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"reportNodeInternalIp":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"scope":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"shareProcessNamespace":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"controller":{"watchIngressWithoutClass":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"autoscaling":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"image":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-nginx":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"egressRules":{"denyConnectionsToExternal":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"metrics":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"pdb":{"create":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"pdb":{"create":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"serviceAccount":{"automountServiceAccountToken":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"serviceBindings":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"mariadb":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"audit":{"logConnections":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"audit":{"logDisconnections":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"audit":{"logHostname":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"ldap":{"tls":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"egressRules":{"denyConnectionsToExternal":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"metrics":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"hostIPC":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"standby":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"psp":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"hostIPC":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"serviceAccount":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"tls":{"autoGenerated":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertmanager":null,"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":false},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"rbac":{"pspEnabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"ingress":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"podDisruptionBudget":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"releaseNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"pvcDeleteOnStsDelete":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"pvcDeleteOnStsScale":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"tcpSocketProbeEnabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"verticalAutoscaler":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"prometheus":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"auth":{"enableLoopbackUser":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"auth":{"tls":{"autoGenerated":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"auth":{"tls":{"existingSecretFullChain":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"clustering":{"forceBoot":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"clustering":{"rebalance":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"extraSecretsPrependReleaseName":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"selfSigned":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"tls":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"authorisationEnabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"skipVerify":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"startTls":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"loadDefinition":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"pdb":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"serviceBindings":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"startupProbe":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"tcpListenOptions":{"keepalive":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"rabbitmq":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"master":{"shareProcessNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"master":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"pdb":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"podSecurityPolicy":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"replica":{"autoscaling":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"replica":{"externalMaster":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"replica":{"shareProcessNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"automateClusterRecovery":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"externalMaster":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"image":{"debug":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"persistence":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sysctl":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"sysctl":{"mountHostSys":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"tls":{"autoGenerated":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"useExternalDNS":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"redis":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: "{}",
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"cainjector":{"containerSecurityContext":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"containerSecurityContext":{"allowPrivilegeEscalation":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"global":{"podSecurityPolicy":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"prometheus":{"servicemonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"prometheus":{"servicemonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"startupapicheck":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"webhook":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true,"webhook":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"cert-manager":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"autoscaling":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"containerSecurityContext":{"allowPrivilegeEscalation":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"downloadDashboards":{"securityContext":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enableKubeBackwardCompatibility":true,"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"headlessService":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"autoscaling":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"networkPolicy":{"limitEgress":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"imageRenderer":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"ingress":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"initChownData":{"securityContext":{"runAsNonRoot":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"egress":{"blockDNSResolution":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"egress":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"persistence":{"disableWarning":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"persistence":{"inMemory":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"rbac":{"namespaced":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"rbac":{"pspUseAppArmor":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"serviceAccount":{"automountServiceAccountToken":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"serviceMonitor":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"initAlerts":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"alerts":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"dashboards":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"dashboards":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"initDatasources":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"datasources":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"enableUniqueFilenames":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"initNotifiers":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"notifiers":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"initPlugins":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"plugins":{"skipReload":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"sidecar":{"securityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true,"useStatefulSet":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"grafana":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"autoscaling":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"hostNetwork":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"hostPort":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"image":{"chroot":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"ingressClassByName":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"ingressClassResource":{"default":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"keda":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"keda":{"restoreToOriginalReplicaCount":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"metrics":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"reportNodeInternalIp":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"scope":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"shareProcessNamespace":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"controller":{"watchIngressWithoutClass":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"autoscaling":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"defaultBackend":{"image":{"allowPrivilegeEscalation":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":true,"rbac":{"scope":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-nginx":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"egressRules":{"denyConnectionsToExternal":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"networkPolicy":{"metrics":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"pdb":{"create":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"primary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"containerSecurityContext":{"allowPrivilegeEscalation":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"containerSecurityContext":{"privileged":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"pdb":{"create":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"secondary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"serviceAccount":{"automountServiceAccountToken":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"serviceBindings":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"audit":{"logConnections":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"audit":{"logDisconnections":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"audit":{"logHostname":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"ldap":{"tls":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"metrics":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"egressRules":{"denyConnectionsToExternal":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"networkPolicy":{"metrics":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"hostIPC":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"standby":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"primary":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"psp":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"hostIPC":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"readReplicas":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"serviceAccount":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"tls":{"autoGenerated":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":false},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":false}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"hostNetwork":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"ingress":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"podDisruptionBudget":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"releaseNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"pvcDeleteOnStsDelete":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"statefulSet":{"pvcDeleteOnStsScale":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"tcpSocketProbeEnabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true,"server":{"verticalAutoscaler":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"prometheus":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"auth":{"enableLoopbackUser":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"auth":{"tls":{"autoGenerated":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"auth":{"tls":{"existingSecretFullChain":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"clustering":{"forceBoot":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"clustering":{"rebalance":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"extraSecretsPrependReleaseName":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"selfSigned":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ingress":{"tls":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"authorisationEnabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"skipVerify":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"ldap":{"tls":{"startTls":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"loadDefinition":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"memoryHighWatermark":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"pdb":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"serviceBindings":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"startupProbe":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"tcpListenOptions":{"keepalive":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"rabbitmq":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"auth":{"usePasswordFiles":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"diagnosticMode":{"enabled":true},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"image":{"debug":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"master":{"shareProcessNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"master":{"startupProbe":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"prometheusRule":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"serviceMonitor":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"metrics":{"serviceMonitor":{"honorLabels":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"networkPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"pdb":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"podSecurityPolicy":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"podSecurityPolicy":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"rbac":{"create":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"replica":{"autoscaling":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"replica":{"externalMaster":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"replica":{"shareProcessNamespace":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"automateClusterRecovery":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"externalMaster":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"image":{"debug":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sentinel":{"persistence":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sysctl":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"sysctl":{"mountHostSys":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"tls":{"autoGenerated":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"useExternalDNS":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true,"volumePermissions":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"redis":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: "{}",
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"prometheus":{"alertmanager":{"enabled":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"prometheus":{"enabled":true,"kube-state-metrics":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-node-exporter":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"prometheus":{"enabled":true,"prometheus-pushgateway":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"alloy":{"crds":{"create":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":{"crds":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"alloy":{"crds":{"create":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"alloy":{"crds":{"create":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"lvm":{"enabled":true}}},"lvm-localpv":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"lvm":{"enabled":true}}},"lvm-localpv":{"crds":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 47: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 48: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":{"crds":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 47: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 48: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":{"crds":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"zfs":{"enabled":true}}},"zfs-localpv":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"local":{"zfs":{"enabled":true}}},"zfs-localpv":{"crds":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 55: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 56: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 57: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"alloy":{"crds":{"create":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"base":{"jaeger":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"crds":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"crds":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"crds":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &[
                "document 44: apiextensions.k8s.io/v1/CustomResourceDefinition zfsrestores.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 45: apiextensions.k8s.io/v1/CustomResourceDefinition zfssnapshots.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
                "document 46: apiextensions.k8s.io/v1/CustomResourceDefinition zfsvolumes.zfs.openebs.io: /apiVersion: apiextensions.k8s.io/v1/CustomResourceDefinition is not served by the pinned Kubernetes version",
            ],
        },
        owner: "oracle: kube-version v3 CRD not-found (21)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"eventing":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"loki":{"enabled":true,"monitoring":{"selfMonitoring":{"grafanaAgent":{"installOperator":null}}}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"loki":{"enabled":true,"monitoring":{"selfMonitoring":{"grafanaAgent":{"installOperator":null}}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"alloy":{"crds":{"create":null},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"engines":{"local":{"rawfile":{"enabled":true}}},"rawfile-localpv":{"crds":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"alloy":{"crds":{"create":null},"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "openebs",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"loki":{"enabled":true,"rollout_operator":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "redmine",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"mariadb":null}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"clickhouse":{"enabled":true,"zookeeper":null}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"clickhouse":{"enabled":true,"zookeeper":{"common":null,"enabled":true}},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"common":null,"enabled":true}},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":true}},"tags":{"bitnami-common":false}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":true}},"tags":{"bitnami-common":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"signoz-otel-gateway":{"enabled":true,"postgresql":{"enabled":null}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "empty array",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "empty object",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "empty object item",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "false",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "integer",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "number",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "true",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "signoz-signoz",
        selector: FailureSelector::Probe {
            probe: Probe {
                path: "global.clusterName",
                value: "unknown object member",
            },
            kubernetes_version: "1.29.0",
        },
        failure: FailureClass::Adjudication {
            reason: "defaults render could not be adjudicated: unusable defaults Kubernetes control: uncertainty: document 34: clickhouse.altinity.com/v1/ClickHouseInstallation adjudication-clickhouse: pinned CRD schema not found",
        },
        owner: "oracle: chart CRDs / crdcheck (C/D)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"enabled":null},"enabled":true},"tags":{"hirosystems-bitcoin-core":true,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"enabled":null},"enabled":true},"tags":{"hirosystems-bitcoin-core":false,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"enabled":null},"enabled":true},"tags":{"hirosystems-bitcoin-core":true,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "stacks-blockchain-api",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"stacks-blockchain":{"bitcoin-core":{"enabled":null},"enabled":true},"tags":{"hirosystems-bitcoin-core":true,"hirosystems-stacks-blockchain":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        },
        owner: "generator: lint-floor null deletion (A)",
        observed_on: "landing10a/d7ab4078",
    },
];
