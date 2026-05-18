//! Classification of Kubernetes API groups as built-in vs. CRD.
//!
//! Single source of truth — used by the CRD catalog provider to skip
//! resources whose schemas live in `kubernetes-json-schema` rather than
//! in a CRD mirror, and available to other code that needs the same
//! distinction.

/// Explicit allowlist of built-in Kubernetes API groups (group names as
/// they appear in `apiVersion`, NOT in URLs). Maintained against the
/// current K8s API reference. Anything not on this list — including
/// CRD groups that happen to use a `.k8s.io` suffix — is treated as a
/// CRD that lives in a CRD catalog.
///
/// CRD groups currently in the wild that use `.k8s.io` and would be
/// misclassified by a naive `.ends_with(".k8s.io")` rule:
///   - `autoscaling.k8s.io` — VPA (`VerticalPodAutoscaler`, …)
///   - `gateway.networking.k8s.io` — Gateway API
///     (`Gateway`, `HTTPRoute`, `GatewayClass`, `ReferenceGrant`, …)
///   - `snapshot.storage.k8s.io` — CSI snapshotter
///     (`VolumeSnapshot`, `VolumeSnapshotClass`,
///     `VolumeSnapshotContent`)
///   - `metrics.k8s.io` — aggregated metrics-server APIs
///     (`PodMetrics`, `NodeMetrics`)
const BUILTIN_K8S_GROUPS: &[&str] = &[
    // Legacy short groups (no `.k8s.io` suffix, predate the convention).
    "apps",
    "batch",
    // `autoscaling` ≠ `autoscaling.k8s.io`. The bare `autoscaling`
    // group holds the built-in HPA; the suffixed group is the VPA CRD.
    "autoscaling",
    "policy",
    "extensions",
    // `.k8s.io` family (sig-managed APIs). Maintained explicitly so
    // CRD groups under `.k8s.io` are NOT swept in.
    "admissionregistration.k8s.io",
    "apiextensions.k8s.io",
    "apiregistration.k8s.io",
    "authentication.k8s.io",
    "authorization.k8s.io",
    "certificates.k8s.io",
    "coordination.k8s.io",
    "discovery.k8s.io",
    "events.k8s.io",
    "flowcontrol.apiserver.k8s.io",
    "internal.apiserver.k8s.io",
    "networking.k8s.io",
    "node.k8s.io",
    "rbac.authorization.k8s.io",
    "resource.k8s.io",
    "scheduling.k8s.io",
    "storage.k8s.io",
];

/// True if `api_group` belongs to the built-in Kubernetes API surface
/// — schemas for these resources live in `kubernetes-json-schema`, not
/// in a CRDs catalog.
///
/// Empty string (the implicit core group — `Pod`, `Service`, …) is
/// considered built-in. Everything else must be on the explicit
/// allowlist.
#[must_use]
pub fn is_k8s_builtin_group(api_group: &str) -> bool {
    let group_lc = api_group.trim().to_ascii_lowercase();
    if group_lc.is_empty() {
        return true;
    }
    BUILTIN_K8S_GROUPS.contains(&group_lc.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_group_is_builtin() {
        assert!(is_k8s_builtin_group(""));
        assert!(is_k8s_builtin_group("  "));
    }

    #[test]
    fn legacy_short_groups_are_builtin() {
        for g in ["apps", "batch", "autoscaling", "policy", "extensions"] {
            assert!(is_k8s_builtin_group(g), "{g} must be built-in");
        }
    }

    #[test]
    fn k8s_io_family_is_builtin() {
        for g in [
            "networking.k8s.io",
            "rbac.authorization.k8s.io",
            "storage.k8s.io",
            "coordination.k8s.io",
            "events.k8s.io",
            "certificates.k8s.io",
            "admissionregistration.k8s.io",
            "apiextensions.k8s.io",
            "flowcontrol.apiserver.k8s.io",
            "node.k8s.io",
            "resource.k8s.io",
            "scheduling.k8s.io",
            "discovery.k8s.io",
        ] {
            assert!(is_k8s_builtin_group(g), "{g} must be built-in");
        }
    }

    #[test]
    fn third_party_crd_groups_are_not_builtin() {
        for g in [
            "monitoring.coreos.com",
            "cert-manager.io",
            "argoproj.io",
            "external-secrets.io",
            "traefik.io",
            "linkerd.io",
            "networking.istio.io",
        ] {
            assert!(!is_k8s_builtin_group(g), "{g} must NOT be built-in");
        }
    }

    // Pins Finding 2 (round 2) — these are CRDs that happen to use a
    // `.k8s.io` suffix. A blanket `.ends_with(".k8s.io")` rule
    // (regressed in PR review round 1) treats them as built-in,
    // skipping their CRD-catalog lookup and producing spurious
    // MissingSchema diagnostics for the resources.
    #[test]
    fn k8s_io_crd_groups_are_not_builtin() {
        for g in [
            // VPA
            "autoscaling.k8s.io",
            // Gateway API
            "gateway.networking.k8s.io",
            // CSI snapshotter
            "snapshot.storage.k8s.io",
            // metrics-server aggregated APIs
            "metrics.k8s.io",
        ] {
            assert!(
                !is_k8s_builtin_group(g),
                "{g} is a CRD/aggregated-API group, NOT a built-in"
            );
        }
    }

    #[test]
    fn case_insensitive() {
        assert!(is_k8s_builtin_group("Apps"));
        assert!(is_k8s_builtin_group("NETWORKING.K8S.IO"));
        assert!(!is_k8s_builtin_group("AUTOSCALING.K8S.IO"));
    }
}
