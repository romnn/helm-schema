//! Accepted documents whose render Helm produces and the pinned evidence
//! cannot decide, that the battery knows.
//!
//! Each accepted render Kubernetes cannot decide is listed with the exact
//! uncertainty the adjudicator reports. The battery fails on a cell missing
//! from the roster and on an entry that is no longer undecided alike, which
//! must be removed. Rows are adjudicated against the same baseline as the
//! false-acceptance roster (`ROSTER_BASELINE`).

use crate::known_false_acceptances::{ContextSelector, ContextTransport, Probe};

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

/// An exact contextual acceptance left undecided for these ordered reasons.
pub(crate) struct ContextUndecidedAcceptance {
    pub(crate) chart: &'static str,
    pub(crate) selector: ContextSelector,
    pub(crate) uncertain: &'static [&'static str],
}

pub(crate) const KNOWN_CONTEXT_UNDECIDED_ACCEPTANCES: &[ContextUndecidedAcceptance] = &[
    // The pinned bundle/catalog lacks the rendered resource versions below, so their
    // changed resources remain undecided despite a successful Helm render.
    ContextUndecidedAcceptance {
        chart: "apisix",
        selector: ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"ingress-controller":{"enabled":true,"gatewayProxy":{"createDefault":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        },
        uncertain: &[
            "document 38: apisix.apache.org/v1alpha1/GatewayProxy adjudication-config: pinned CRD schema not found",
        ],
    },
    // The pinned bundle/catalog lacks the rendered resource versions below, so their
    // changed resources remain undecided despite a successful Helm render.
    ContextUndecidedAcceptance {
        chart: "apisix",
        selector: ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"ingress-controller":{"enabled":true,"gatewayProxy":{"createDefault":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        },
        uncertain: &[
            "document 38: apisix.apache.org/v1alpha1/GatewayProxy adjudication-config: pinned CRD schema not found",
        ],
    },
    // The pinned bundle/catalog lacks the rendered resource versions below, so their
    // changed resources remain undecided despite a successful Helm render.
    ContextUndecidedAcceptance {
        chart: "apisix",
        selector: ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::Template,
            overlay: r#"{"ingress-controller":{"enabled":true,"gatewayProxy":{"createDefault":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        },
        uncertain: &[
            "document 38: apisix.apache.org/v1alpha1/GatewayProxy adjudication-config: pinned CRD schema not found",
        ],
    },
    // The pinned bundle/catalog lacks the rendered resource versions below, so their
    // changed resources remain undecided despite a successful Helm render.
    ContextUndecidedAcceptance {
        chart: "milvus",
        selector: ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"pulsar":{"certs":{"internal_issuer":{"enabled":true}},"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        },
        uncertain: &[
            "document 90: cert-manager.io/v1alpha2/Certificate adjudication-pulsar-ca: pinned CRD schema not found",
            "document 91: cert-manager.io/v1alpha2/Issuer adjudication-pulsar-internal-cert-issuer: pinned CRD schema not found",
            "document 92: cert-manager.io/v1alpha2/Issuer adjudication-pulsar-internal-cert-issuer-ca-issuer: pinned CRD schema not found",
        ],
    },
    // The pinned bundle/catalog lacks the rendered resource versions below, so their
    // changed resources remain undecided despite a successful Helm render.
    ContextUndecidedAcceptance {
        chart: "openebs",
        selector: ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"engines":{"replicated":{"mayastor":{"enabled":true}}},"mayastor":{"base":{"jaeger":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        },
        uncertain: &[
            "document 113: jaegertracing.io/v1/Jaeger jaeger: pinned CRD schema not found",
        ],
    },
    // The pinned bundle/catalog lacks the rendered resource versions below, so their
    // changed resources remain undecided despite a successful Helm render.
    ContextUndecidedAcceptance {
        chart: "openebs",
        selector: ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"loki":{"enabled":true,"monitoring":{"selfMonitoring":{"enabled":true}}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        },
        uncertain: &[
            "document 106: monitoring.grafana.com/v1alpha1/GrafanaAgent adjudication-loki: pinned CRD schema not found",
            "document 107: monitoring.grafana.com/v1alpha1/LogsInstance adjudication-loki: pinned CRD schema not found",
            "document 108: monitoring.grafana.com/v1alpha1/PodLogs adjudication-loki: pinned CRD schema not found",
        ],
    },
    // The pinned bundle/catalog lacks the rendered resource versions below, so their
    // changed resources remain undecided despite a successful Helm render.
    ContextUndecidedAcceptance {
        chart: "openebs",
        selector: ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"loki":{"enabled":true,"rbac":{"sccEnabled":true}}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        },
        uncertain: &[
            "document 105: security.openshift.io/v1/SecurityContextConstraints loki: pinned CRD schema not found",
        ],
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
