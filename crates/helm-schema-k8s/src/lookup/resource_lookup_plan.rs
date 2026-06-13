use helm_schema_ir::ResourceRef;

use crate::capability_eval::{self, CapabilityOracle};
use crate::ordered_api_versions_for_resource;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceLookupPlan {
    candidates: Vec<ResourceRef>,
}

impl ResourceLookupPlan {
    pub(crate) fn for_resource<O: CapabilityOracle + ?Sized>(
        resource: &ResourceRef,
        oracle: &O,
    ) -> Self {
        let api_versions = candidate_api_versions(resource, oracle);
        let candidates = api_versions
            .into_iter()
            .map(|api_version| ResourceRef {
                api_version,
                kind: resource.kind.clone(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            })
            .collect();

        Self { candidates }
    }

    pub(crate) fn candidates(&self) -> &[ResourceRef] {
        &self.candidates
    }
}

fn candidate_api_versions<O: CapabilityOracle + ?Sized>(
    resource: &ResourceRef,
    oracle: &O,
) -> Vec<String> {
    if !resource.api_version_branches.is_empty() {
        let live = capability_eval::live_literals(&resource.api_version_branches, oracle);
        if !live.is_empty() {
            return live;
        }
    }

    ordered_api_versions_for_resource(resource)
        .into_iter()
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use helm_schema_ir::{CapabilityGuard, HelperBranch};

    use super::*;
    use crate::capability_eval::StaticOracle;

    fn resource(
        api_version: &str,
        candidates: &[&str],
        branches: Vec<HelperBranch>,
    ) -> ResourceRef {
        ResourceRef {
            api_version: api_version.to_string(),
            kind: "Ingress".to_string(),
            api_version_candidates: candidates
                .iter()
                .map(|candidate| (*candidate).to_string())
                .collect(),
            api_version_branches: branches,
        }
    }

    fn branch_has(api: &str, literals: &[&str]) -> HelperBranch {
        HelperBranch::with_literals(
            Some(CapabilityGuard::Has {
                api: api.to_string(),
            }),
            literals
                .iter()
                .map(|literal| (*literal).to_string())
                .collect(),
        )
    }

    fn branch_else(literals: &[&str]) -> HelperBranch {
        HelperBranch::with_literals(
            None,
            literals
                .iter()
                .map(|literal| (*literal).to_string())
                .collect(),
        )
    }

    fn planned_api_versions(plan: &ResourceLookupPlan) -> Vec<String> {
        plan.candidates()
            .iter()
            .map(|candidate| candidate.api_version.clone())
            .collect()
    }

    #[test]
    fn explicit_candidates_are_ranked_for_resolution() {
        let resource = resource("extensions/v1beta1", &["networking.k8s.io/v1"], Vec::new());
        let plan = ResourceLookupPlan::for_resource(&resource, &StaticOracle::new());

        assert_eq!(
            planned_api_versions(&plan),
            vec!["networking.k8s.io/v1", "extensions/v1beta1"]
        );
    }

    #[test]
    fn live_branch_literals_override_flat_candidates() {
        let resource = resource(
            "networking.k8s.io/v1beta1",
            &["networking.k8s.io/v1"],
            vec![
                branch_has("networking.k8s.io/v1/Ingress", &["networking.k8s.io/v1"]),
                branch_else(&["networking.k8s.io/v1beta1"]),
            ],
        );
        let oracle = StaticOracle::new().with("networking.k8s.io/v1/Ingress", true);
        let plan = ResourceLookupPlan::for_resource(&resource, &oracle);

        assert_eq!(planned_api_versions(&plan), vec!["networking.k8s.io/v1"]);
    }

    #[test]
    fn unresolved_branches_fall_back_to_ranked_candidates() {
        let resource = resource(
            "extensions/v1beta1",
            &["networking.k8s.io/v1"],
            vec![branch_has("networking.k8s.io/v1/Ingress", &[])],
        );
        let oracle = StaticOracle::new().with("networking.k8s.io/v1/Ingress", true);
        let plan = ResourceLookupPlan::for_resource(&resource, &oracle);

        assert_eq!(
            planned_api_versions(&plan),
            vec!["networking.k8s.io/v1", "extensions/v1beta1"]
        );
    }
}
