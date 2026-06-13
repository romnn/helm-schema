use helm_schema_ir::ResourceRef;

/// Declarative probe table for `.Capabilities.APIVersions.Has "group/version"`.
///
/// Helm's capability API accepts both api-version and resource-qualified forms.
/// Resource-qualified probes (`group/version/Kind` or core `version/Kind`) are
/// already exact and do not use this table. Api-version-only probes need one
/// canonical kind whose presence proves that api version exists in the
/// configured K8s schema bundle.
#[derive(Debug, Clone, Copy)]
pub(super) struct CapabilityProbeTable {
    entries: &'static [(&'static str, &'static str)],
}

pub(super) const DEFAULT_CAPABILITY_PROBE_TABLE: CapabilityProbeTable = CapabilityProbeTable {
    entries: WELL_KNOWN_API_VERSION_PROBES,
};

impl CapabilityProbeTable {
    /// Build the `ResourceRef` to probe for a Helm capability literal.
    ///
    /// For `group/version/Kind` and core `version/Kind`, the kind is probed
    /// directly. For `group/version` or core `version`, the declarative table
    /// supplies the canonical probe kind. Unknown api-version-only literals
    /// return `None` so the caller can keep the capability guard potentially
    /// live.
    pub(super) fn build_probe(self, api: &str) -> Option<ResourceRef> {
        let parts: Vec<&str> = api.split('/').collect();
        let (api_version, kind) = match parts.as_slice() {
            [_, _, kind] => (parts[..2].join("/"), (*kind).to_string()),
            [version, kind] if is_k8s_api_version_segment(version) => {
                ((*version).to_string(), (*kind).to_string())
            }
            [_, _] | [_] => (api.to_string(), self.canonical_kind(api)?.to_string()),
            _ => return None,
        };
        if api_version.is_empty() || kind.is_empty() {
            return None;
        }
        Some(ResourceRef {
            api_version,
            kind,
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        })
    }

    fn canonical_kind(self, api_version: &str) -> Option<&'static str> {
        self.entries
            .iter()
            .find(|(candidate, _)| *candidate == api_version)
            .map(|(_, kind)| *kind)
    }
}

fn is_k8s_api_version_segment(segment: &str) -> bool {
    let Some(rest) = segment.strip_prefix('v') else {
        return false;
    };
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return false;
    }
    let suffix = &rest[digit_count..];
    if suffix.is_empty() {
        return true;
    }
    for qualifier in ["alpha", "beta"] {
        if let Some(number) = suffix.strip_prefix(qualifier) {
            return !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
        }
    }
    false
}

const WELL_KNOWN_API_VERSION_PROBES: &[(&str, &str)] = &[
    ("v1", "ConfigMap"),
    ("apps/v1", "Deployment"),
    ("apps/v1beta1", "Deployment"),
    ("apps/v1beta2", "Deployment"),
    ("batch/v1", "Job"),
    ("batch/v1beta1", "CronJob"),
    ("rbac.authorization.k8s.io/v1", "Role"),
    ("rbac.authorization.k8s.io/v1beta1", "Role"),
    ("rbac.authorization.k8s.io/v1alpha1", "Role"),
    ("networking.k8s.io/v1", "Ingress"),
    ("networking.k8s.io/v1beta1", "Ingress"),
    ("extensions/v1beta1", "Ingress"),
    ("policy/v1", "PodDisruptionBudget"),
    ("policy/v1beta1", "PodDisruptionBudget"),
    ("autoscaling/v1", "HorizontalPodAutoscaler"),
    ("autoscaling/v2", "HorizontalPodAutoscaler"),
    ("autoscaling/v2beta1", "HorizontalPodAutoscaler"),
    ("autoscaling/v2beta2", "HorizontalPodAutoscaler"),
    ("storage.k8s.io/v1", "StorageClass"),
    ("storage.k8s.io/v1beta1", "StorageClass"),
    ("apiextensions.k8s.io/v1", "CustomResourceDefinition"),
    ("apiextensions.k8s.io/v1beta1", "CustomResourceDefinition"),
    (
        "admissionregistration.k8s.io/v1",
        "MutatingWebhookConfiguration",
    ),
    (
        "admissionregistration.k8s.io/v1beta1",
        "MutatingWebhookConfiguration",
    ),
    ("scheduling.k8s.io/v1", "PriorityClass"),
    ("scheduling.k8s.io/v1beta1", "PriorityClass"),
    ("coordination.k8s.io/v1", "Lease"),
    ("coordination.k8s.io/v1beta1", "Lease"),
    ("node.k8s.io/v1", "RuntimeClass"),
    ("node.k8s.io/v1beta1", "RuntimeClass"),
    ("discovery.k8s.io/v1", "EndpointSlice"),
    ("discovery.k8s.io/v1beta1", "EndpointSlice"),
    ("events.k8s.io/v1", "Event"),
    ("events.k8s.io/v1beta1", "Event"),
    ("certificates.k8s.io/v1", "CertificateSigningRequest"),
    ("certificates.k8s.io/v1beta1", "CertificateSigningRequest"),
    ("authentication.k8s.io/v1", "TokenReview"),
    ("authorization.k8s.io/v1", "SubjectAccessReview"),
    ("flowcontrol.apiserver.k8s.io/v1", "FlowSchema"),
    ("flowcontrol.apiserver.k8s.io/v1beta3", "FlowSchema"),
    ("flowcontrol.apiserver.k8s.io/v1beta2", "FlowSchema"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(api: &str) -> Option<ResourceRef> {
        DEFAULT_CAPABILITY_PROBE_TABLE.build_probe(api)
    }

    #[test]
    fn group_version_probe_uses_canonical_kind_table() {
        let probe = probe("policy/v1").expect("policy/v1 should have a canonical probe");

        assert_eq!(probe.api_version, "policy/v1");
        assert_eq!(probe.kind, "PodDisruptionBudget");
    }

    #[test]
    fn core_version_probe_uses_canonical_kind_table() {
        let probe = probe("v1").expect("core v1 should have a canonical probe");

        assert_eq!(probe.api_version, "v1");
        assert_eq!(probe.kind, "ConfigMap");
    }

    #[test]
    fn resource_qualified_probe_bypasses_canonical_kind_table() {
        let probe = probe("policy/v1/PodSecurityPolicy").expect("resource probe should be direct");

        assert_eq!(probe.api_version, "policy/v1");
        assert_eq!(probe.kind, "PodSecurityPolicy");
    }

    #[test]
    fn core_resource_qualified_probe_bypasses_canonical_kind_table() {
        let probe = probe("v1/Secret").expect("core resource probe should be direct");

        assert_eq!(probe.api_version, "v1");
        assert_eq!(probe.kind, "Secret");
    }

    #[test]
    fn unknown_group_version_probe_abstains() {
        assert!(probe("example.com/v1").is_none());
    }

    #[test]
    fn malformed_resource_qualified_probe_abstains() {
        assert!(probe("policy/v1/").is_none());
        assert!(probe("v1/").is_none());
    }

    #[test]
    fn api_version_segment_parser_accepts_stable_and_prerelease_versions() {
        assert!(is_k8s_api_version_segment("v1"));
        assert!(is_k8s_api_version_segment("v2beta1"));
        assert!(is_k8s_api_version_segment("v3alpha2"));
    }

    #[test]
    fn api_version_segment_parser_rejects_group_names_and_incomplete_versions() {
        assert!(!is_k8s_api_version_segment("policy"));
        assert!(!is_k8s_api_version_segment("v"));
        assert!(!is_k8s_api_version_segment("v1gamma1"));
    }
}
