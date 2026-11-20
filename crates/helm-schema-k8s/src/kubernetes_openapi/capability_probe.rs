use helm_schema_core::{ApiPresenceQuery, ResourceRef};
use serde_json::Value;

use crate::builtin_groups::is_k8s_builtin_group;
use crate::group_version::GroupVersion;

/// Builds the [`ResourceRef`] to probe for a built-in Helm capability literal.
///
/// Resource-qualified queries probe the kind directly, while API-version-only queries use the
/// declarative canonical-kind table.
/// Malformed apiVersions return `None` (uncertain): they name no probe target.
/// Third-party API groups return `None` because absence from the Kubernetes bundle says nothing
/// about CRDs installed in the target cluster.
pub(super) fn build_capability_probe(query: &ApiPresenceQuery) -> Option<ResourceRef> {
    match query {
        ApiPresenceQuery::Resource { api_version, kind } => {
            let group = GroupVersion::parse(api_version)?.group;
            is_k8s_builtin_group(group)
                .then(|| ResourceRef::concrete(api_version.clone(), kind.clone()))
        }
        ApiPresenceQuery::GroupVersion { api_version } => Some(ResourceRef::concrete(
            api_version.clone(),
            canonical_kind(api_version)?.to_string(),
        )),
    }
}

/// The release API catalog every bundle directory of one release derives from.
pub(super) const RELEASE_CATALOG: &str = "_definitions.json";

/// Bundle variants published beside each release directory. Their catalogs declare the same
/// kinds as the release's, and the `-strict` variant publishes none, so every variant reads the
/// one release catalog (the one a vendored bundle keeps).
const BUNDLE_VARIANT_SUFFIXES: &[&str] =
    &["-standalone-strict", "-standalone", "-strict", "-local"];

/// The release directory publishing the API catalog for `version_dir`
/// (`v1.29.0-standalone-strict` → `v1.29.0`).
pub(super) fn catalog_release(version_dir: &str) -> &str {
    BUNDLE_VARIANT_SUFFIXES
        .iter()
        .find_map(|suffix| version_dir.strip_suffix(suffix))
        .unwrap_or(version_dir)
}

/// The definitions that declare `probe`'s exact group, version and kind as their single root.
///
/// An empty result records a valid omission. A malformed catalog gives no evidence of absence.
pub(super) fn catalog_declarations<'a>(
    catalog: &'a Value,
    probe: &ResourceRef,
) -> Option<Vec<&'a str>> {
    let target = GroupVersion::parse(&probe.api_version)?;
    let definitions = catalog.get("definitions")?.as_object()?;
    let mut declarations = Vec::new();
    for (name, definition) in definitions {
        let Some(entries) = definition
            .as_object()?
            .get("x-kubernetes-group-version-kind")
        else {
            continue;
        };
        let entries = entries.as_array()?;
        for entry in entries {
            let group = entry.get("group")?.as_str()?;
            let version = entry.get("version")?.as_str()?;
            let kind = entry.get("kind")?.as_str()?;
            // Generic meta types list multiple group/versions, including retired ones.
            if entries.len() == 1
                && group == target.group
                && version == target.version
                && kind == probe.kind
            {
                declarations.push(name.as_str());
            }
        }
    }
    Some(declarations)
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
