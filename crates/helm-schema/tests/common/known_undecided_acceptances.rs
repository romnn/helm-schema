//! Accepted documents whose render Helm produces and the pinned evidence
//! cannot decide, that the battery knows.
//!
//! Each accepted render Kubernetes cannot decide is listed with the exact
//! uncertainty the adjudicator reports. The battery fails on a cell missing
//! from the roster and on an entry that is no longer undecided alike, which
//! must be removed. Rows are adjudicated against the same baseline as the
//! false-acceptance roster (`ROSTER_BASELINE`).

use crate::known_false_acceptances::Probe;

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
