use helm_schema_ir::ResourceRef;

fn parse_k8s_semver(version_dir: &str) -> Option<(u32, u32, u32)> {
    let v = version_dir.trim().trim_start_matches('v');
    let v = v.split('-').next().unwrap_or(v);
    let mut it = v.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next()?.parse().ok()?;
    let patch = it.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// Returns a short user-facing hint for resources we know were removed
/// from a specific K8s minor — e.g. HPA `autoscaling/v2beta1` going
/// away in v1.25.
#[must_use]
pub fn missing_schema_hint(resource: &ResourceRef) -> Option<String> {
    // The hint requires a current k8s version to anchor "before X" / "after X" claims.
    // The chain layer doesn't pass it through, so for now produce only
    // version-independent removals (the ones the original code rendered against
    // primary v1.35.0, which always triggers).
    if resource.kind == "HorizontalPodAutoscaler" && resource.api_version == "autoscaling/v2beta1" {
        return Some(
            "autoscaling/v2beta1 HorizontalPodAutoscaler was removed in Kubernetes v1.25+"
                .to_string(),
        );
    }
    None
}

/// Variant that takes the K8s version_dir as well, for use by code
/// that has the primary version handy. Kept for parity with the
/// pre-refactor signature.
#[must_use]
pub fn missing_schema_hint_for_version(
    version_dir: &str,
    resource: &ResourceRef,
) -> Option<String> {
    let (major, minor, _patch) = parse_k8s_semver(version_dir)?;
    if resource.kind == "HorizontalPodAutoscaler"
        && resource.api_version == "autoscaling/v2beta1"
        && major == 1
        && minor >= 25
    {
        return Some(
            "autoscaling/v2beta1 HorizontalPodAutoscaler was removed in Kubernetes v1.25+"
                .to_string(),
        );
    }
    None
}
