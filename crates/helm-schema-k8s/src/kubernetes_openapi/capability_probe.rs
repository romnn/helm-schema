use helm_schema_core::{ApiPresenceQuery, ResourceRef};

/// Build the `ResourceRef` to probe for a Helm capability literal.
///
/// For `group/version/Kind` and core `version/Kind`, the kind is probed
/// directly. For `group/version` or core `version`, the declarative table
/// supplies the canonical probe kind. Unknown api-version-only literals
/// return `None` so the caller can keep the capability guard potentially
/// live.
pub(super) fn build_capability_probe(query: &ApiPresenceQuery) -> Option<ResourceRef> {
    match query {
        ApiPresenceQuery::Resource { api_version, kind } => {
            Some(ResourceRef::concrete(api_version.clone(), kind.clone()))
        }
        ApiPresenceQuery::GroupVersion { api_version } => Some(ResourceRef::concrete(
            api_version.clone(),
            canonical_kind(api_version)?.to_string(),
        )),
    }
}

fn canonical_kind(api_version: &str) -> Option<&'static str> {
    WELL_KNOWN_API_VERSION_PROBES
        .iter()
        .find(|(candidate, _)| *candidate == api_version)
        .map(|(_, kind)| *kind)
}

/// Declarative probe table for `.Capabilities.APIVersions.Has "group/version"`.
///
/// Helm's capability API accepts both api-version and resource-qualified forms.
/// Resource-qualified probes (`group/version/Kind` or core `version/Kind`) are
/// already exact and do not use this table. Api-version-only probes need one
/// canonical kind whose presence proves that api version exists in the
/// configured K8s schema bundle.
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
#[path = "tests/capability_probe.rs"]
mod tests;
