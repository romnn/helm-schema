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
