use helm_schema_core::{CapabilityOracle, ResourceRef, live_literals};

use crate::ordered_api_versions_for_resource;

pub(crate) fn resource_lookup_candidates<O: CapabilityOracle + ?Sized>(
    resource: &ResourceRef,
    oracle: &O,
) -> Vec<ResourceRef> {
    let api_versions = candidate_api_versions(resource, oracle);
    resource_candidates_with_api_versions(resource, api_versions)
}

pub(crate) fn missing_schema_attribution_candidates<O: CapabilityOracle + ?Sized>(
    resource: &ResourceRef,
    oracle: &O,
) -> Vec<ResourceRef> {
    let api_versions = missing_schema_attribution_api_versions(resource, oracle);
    resource_candidates_with_api_versions(resource, api_versions)
}

fn candidate_api_versions<O: CapabilityOracle + ?Sized>(
    resource: &ResourceRef,
    oracle: &O,
) -> Vec<String> {
    if !resource.api_version_branches.is_empty() {
        let live = live_literals(&resource.api_version_branches, oracle);
        if !live.is_empty() {
            return live;
        }
    }

    ordered_api_versions_for_resource(resource)
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn missing_schema_attribution_api_versions<O: CapabilityOracle + ?Sized>(
    resource: &ResourceRef,
    oracle: &O,
) -> Vec<String> {
    if !resource.api_version_branches.is_empty() {
        let live = live_literals(&resource.api_version_branches, oracle);
        return match live.first().cloned() {
            Some(api_version) => vec![api_version],
            None if resource.api_version.is_empty()
                && !resource.api_version_candidates.is_empty() =>
            {
                resource.api_version_candidates.clone()
            }
            None => vec![resource.api_version.clone()],
        };
    }

    if resource.api_version.is_empty() && !resource.api_version_candidates.is_empty() {
        return resource.api_version_candidates.clone();
    }

    vec![resource.api_version.clone()]
}

fn resource_candidates_with_api_versions(
    resource: &ResourceRef,
    api_versions: Vec<String>,
) -> Vec<ResourceRef> {
    api_versions
        .into_iter()
        .map(|api_version| ResourceRef {
            api_version,
            kind: resource.kind.clone(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/resource_lookup_plan.rs"]
mod tests;
