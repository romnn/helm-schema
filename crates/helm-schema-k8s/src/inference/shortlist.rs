/// Hardcoded canonical `kind → apiVersion` mapping for kinds where the
/// apiVersion is unambiguous in practice. Reviewed once per K8s major.
///
/// Entries with known ambiguity (e.g. `Ingress` — both
/// `networking.k8s.io/v1` and `extensions/v1beta1` exist) are
/// deliberately omitted so we don't silently pick the wrong one.
const KIND_TO_API_VERSION: &[(&str, &str)] = &[
    // Core API (v1)
    ("Pod", "v1"),
    ("ConfigMap", "v1"),
    ("Secret", "v1"),
    ("Service", "v1"),
    ("Namespace", "v1"),
    ("Node", "v1"),
    ("PersistentVolume", "v1"),
    ("PersistentVolumeClaim", "v1"),
    ("ServiceAccount", "v1"),
    ("Endpoints", "v1"),
    ("Event", "v1"),
    ("LimitRange", "v1"),
    ("ResourceQuota", "v1"),
    ("ReplicationController", "v1"),
    // apps/v1
    ("Deployment", "apps/v1"),
    ("StatefulSet", "apps/v1"),
    ("DaemonSet", "apps/v1"),
    ("ReplicaSet", "apps/v1"),
    // batch/v1
    ("Job", "batch/v1"),
    ("CronJob", "batch/v1"),
    // networking.k8s.io/v1 — `Ingress` deliberately omitted (extensions/v1beta1 ambiguity)
    ("NetworkPolicy", "networking.k8s.io/v1"),
    ("IngressClass", "networking.k8s.io/v1"),
    // rbac.authorization.k8s.io/v1
    ("Role", "rbac.authorization.k8s.io/v1"),
    ("RoleBinding", "rbac.authorization.k8s.io/v1"),
    ("ClusterRole", "rbac.authorization.k8s.io/v1"),
    ("ClusterRoleBinding", "rbac.authorization.k8s.io/v1"),
    // `PodDisruptionBudget` is DELIBERATELY omitted from the shortlist:
    // it lived at `policy/v1beta1` until v1.21 and at `policy/v1`
    // afterward. With `--k8s-version-fallback=auto` enabled, both
    // versions can co-exist in the cache (the primary version's file
    // plus the fallback version's file), and the cache scan tier
    // would surface a distinct set of two api_versions for the same
    // kind → `AmbiguousApiVersion`. Inference for a kind that lacks a
    // YAML-level `apiVersion` falls back to the cache scan tier
    // alone, which still produces the right answer when only the
    // primary version dir holds the file (see the cache-scan
    // explicit-versions filter).
    // storage.k8s.io/v1
    ("StorageClass", "storage.k8s.io/v1"),
    // External — the cases that matter for Temporal
    ("ServiceMonitor", "monitoring.coreos.com/v1"),
    ("PodMonitor", "monitoring.coreos.com/v1"),
    ("PrometheusRule", "monitoring.coreos.com/v1"),
];

/// Look up the canonical apiVersion for a kind in the shortlist.
#[must_use]
pub fn canonical_api_version_for_kind(kind: &str) -> Option<&'static str> {
    KIND_TO_API_VERSION
        .iter()
        .find_map(|(k, v)| (*k == kind).then_some(*v))
}

/// Extended `kind → (canonical_group, canonical_version)` mapping for
/// the online probe tier. Only kinds we know exactly which group they
/// belong to. Tier 3 abstains for kinds not in this table.
#[must_use]
pub fn canonical_group_version_for_kind(kind: &str) -> Option<(&'static str, &'static str)> {
    let api_version = canonical_api_version_for_kind(kind)?;
    if let Some((group, version)) = api_version.split_once('/') {
        Some((group, version))
    } else {
        // Core API: empty group, version-only.
        Some(("", api_version))
    }
}
