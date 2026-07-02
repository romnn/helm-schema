use super::*;
use test_util::prelude::sim_assert_eq;

fn resource(api_version: &str, kind: &str) -> ResourceRef {
    ResourceRef::concrete(api_version.to_string(), kind.to_string())
}

#[test]
fn third_party_crds_yield_relative_path() {
    let r = resource("monitoring.coreos.com/v1", "ServiceMonitor");
    sim_assert_eq!(
        have: relative_path_for_resource(&r),
        want: Some("monitoring.coreos.com/servicemonitor_v1.json".to_string())
    );
}

// Pins Finding 3 — every built-in K8s API group must be excluded
// from the CRD-catalog lookup path. Regression on this surface
// produces spurious CrdVersionNotFound diagnostics for built-in
// grouped resources (Ingress, NetworkPolicy, Role, RoleBinding,
// ClusterRole, ClusterRoleBinding, …).
#[test]
fn k8s_io_family_is_excluded_from_crd_catalog() {
    for (api_version, kind) in [
        ("networking.k8s.io/v1", "Ingress"),
        ("networking.k8s.io/v1", "NetworkPolicy"),
        ("rbac.authorization.k8s.io/v1", "Role"),
        ("rbac.authorization.k8s.io/v1", "RoleBinding"),
        ("rbac.authorization.k8s.io/v1", "ClusterRole"),
        ("rbac.authorization.k8s.io/v1", "ClusterRoleBinding"),
        ("storage.k8s.io/v1", "StorageClass"),
        ("coordination.k8s.io/v1", "Lease"),
        ("events.k8s.io/v1", "Event"),
        (
            "admissionregistration.k8s.io/v1",
            "ValidatingWebhookConfiguration",
        ),
        ("apiextensions.k8s.io/v1", "CustomResourceDefinition"),
    ] {
        sim_assert_eq!(
            have: relative_path_for_resource(&resource(api_version, kind)),
            want: None,
            "{api_version}/{kind} must NOT be routed to the CRD catalog"
        );
    }
}

#[test]
fn legacy_short_groups_are_excluded() {
    for (api_version, kind) in [
        ("apps/v1", "Deployment"),
        ("batch/v1", "Job"),
        ("autoscaling/v2", "HorizontalPodAutoscaler"),
        ("policy/v1", "PodDisruptionBudget"),
        ("policy/v1beta1", "PodSecurityPolicy"),
        ("extensions/v1beta1", "DaemonSet"),
    ] {
        sim_assert_eq!(
            have: relative_path_for_resource(&resource(api_version, kind)),
            want: None,
            "{api_version}/{kind} must NOT be routed to the CRD catalog"
        );
    }
}

// Pins Finding 2 (round 2) — CRDs that use a `.k8s.io` suffix must
// resolve to a relative path so the CRD-catalog lookup runs for
// them. The previously-shipped blanket `.ends_with(".k8s.io")`
// rule routed them to the built-in K8s path and produced spurious
// MissingSchema in the loose-mode Temporal acceptance run.
#[test]
fn k8s_io_crd_groups_yield_relative_path() {
    // VPA — the canonical `.k8s.io`-suffixed CRD.
    sim_assert_eq!(
        have: relative_path_for_resource(&resource("autoscaling.k8s.io/v1", "VerticalPodAutoscaler")),
        want: Some("autoscaling.k8s.io/verticalpodautoscaler_v1.json".to_string())
    );
    // Gateway API — the canonical multi-resource CRD family
    // under `.k8s.io`.
    sim_assert_eq!(
        have: relative_path_for_resource(&resource("gateway.networking.k8s.io/v1", "HTTPRoute")),
        want: Some("gateway.networking.k8s.io/httproute_v1.json".to_string())
    );
    sim_assert_eq!(
        have: relative_path_for_resource(&resource("gateway.networking.k8s.io/v1", "Gateway")),
        want: Some("gateway.networking.k8s.io/gateway_v1.json".to_string())
    );
    // CSI snapshotter — distinct from the built-in
    // `storage.k8s.io` group.
    sim_assert_eq!(
        have: relative_path_for_resource(&resource("snapshot.storage.k8s.io/v1", "VolumeSnapshot")),
        want: Some("snapshot.storage.k8s.io/volumesnapshot_v1.json".to_string())
    );
}
