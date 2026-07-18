use std::collections::BTreeMap;

use helm_schema_core::{
    ApiPresenceQuery, CapabilityGuard, CapabilityOracle, HelperBranch, HelperBranchBody,
};
use test_util::prelude::sim_assert_eq;

use super::*;

#[derive(Default)]
struct StaticOracle {
    answers: BTreeMap<String, bool>,
}

impl StaticOracle {
    fn new() -> Self {
        Self::default()
    }

    fn with(mut self, api: &str, has: bool) -> Self {
        if let Some(query) = ApiPresenceQuery::parse_helm_literal(api) {
            self.answers.insert(query.canonical_helm_literal(), has);
        }
        self
    }
}

impl CapabilityOracle for StaticOracle {
    fn capability_has_query(&self, query: &ApiPresenceQuery) -> Option<bool> {
        self.answers.get(&query.canonical_helm_literal()).copied()
    }
}

fn resource(api_version: &str, candidates: &[&str], branches: Vec<HelperBranch>) -> ResourceRef {
    ResourceRef {
        api_version: api_version.to_string(),
        kind: "Ingress".to_string(),
        kind_candidates: Vec::new(),
        api_version_candidates: candidates
            .iter()
            .map(|candidate| (*candidate).to_string())
            .collect(),
        api_version_branches: branches,
        kind_branches: Vec::new(),
    }
}

fn branch_has(api: &str, literals: &[&str]) -> HelperBranch {
    HelperBranch {
        guard: Some(CapabilityGuard::Has {
            api: api.to_string(),
        }),
        body: literal_body(literals),
    }
}

fn branch_else(literals: &[&str]) -> HelperBranch {
    HelperBranch {
        guard: None,
        body: literal_body(literals),
    }
}

fn branch_opaque(literals: &[&str]) -> HelperBranch {
    HelperBranch {
        guard: Some(CapabilityGuard::Opaque {
            text: "semverCompare \"<1.19-0\" .Capabilities.KubeVersion.GitVersion".to_string(),
        }),
        body: literal_body(literals),
    }
}

fn literal_body(literals: &[&str]) -> HelperBranchBody {
    HelperBranchBody::Literals {
        values: literals
            .iter()
            .map(|literal| (*literal).to_string())
            .collect(),
    }
}

fn planned_api_versions(candidates: &[ResourceRef]) -> Vec<String> {
    candidates
        .iter()
        .map(|candidate| candidate.api_version.clone())
        .collect()
}

#[test]
fn explicit_candidates_are_ranked_for_resolution() {
    let resource = resource("extensions/v1beta1", &["networking.k8s.io/v1"], Vec::new());
    let candidates = resource_lookup_candidates(&resource, &StaticOracle::new());

    sim_assert_eq!(
        have: planned_api_versions(&candidates),
        want: vec!["networking.k8s.io/v1", "extensions/v1beta1"]
    );
}

#[test]
fn finite_kind_candidates_cross_with_api_version_candidates() {
    let mut resource = resource("apps/v1", &[], Vec::new());
    resource.kind = "StatefulSet".to_string();
    resource.kind_candidates = vec!["Deployment".to_string()];
    let candidates = resource_lookup_candidates(&resource, &StaticOracle::new());

    sim_assert_eq!(
        have: candidates
            .into_iter()
            .map(|candidate| (candidate.api_version, candidate.kind))
            .collect::<Vec<_>>(),
        want: vec![
            ("apps/v1".to_string(), "StatefulSet".to_string()),
            ("apps/v1".to_string(), "Deployment".to_string()),
        ]
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
    let candidates = resource_lookup_candidates(&resource, &oracle);

    sim_assert_eq!(have: planned_api_versions(&candidates), want: vec!["networking.k8s.io/v1"]);
}

#[test]
fn unresolved_branches_fall_back_to_ranked_candidates() {
    let resource = resource(
        "extensions/v1beta1",
        &["networking.k8s.io/v1"],
        vec![branch_has("networking.k8s.io/v1/Ingress", &[])],
    );
    let oracle = StaticOracle::new().with("networking.k8s.io/v1/Ingress", true);
    let candidates = resource_lookup_candidates(&resource, &oracle);

    sim_assert_eq!(
        have: planned_api_versions(&candidates),
        want: vec!["networking.k8s.io/v1", "extensions/v1beta1"]
    );
}

#[test]
fn opaque_branch_guard_falls_back_to_ranked_candidates() {
    let resource = resource(
        "extensions/v1beta1",
        &["networking.k8s.io/v1"],
        vec![
            branch_opaque(&["extensions/v1beta1"]),
            branch_else(&["networking.k8s.io/v1"]),
        ],
    );
    let candidates = resource_lookup_candidates(&resource, &StaticOracle::new());

    sim_assert_eq!(
        have: planned_api_versions(&candidates),
        want: vec!["networking.k8s.io/v1", "extensions/v1beta1"]
    );
}

#[test]
fn missing_attribution_uses_first_live_branch_literal_only() {
    let resource = resource(
        "",
        &["networking.k8s.io/v1beta1", "networking.k8s.io/v1"],
        vec![
            branch_has(
                "networking.k8s.io/v1/Ingress",
                &["networking.k8s.io/v1", "networking.k8s.io/v1beta1"],
            ),
            branch_else(&["extensions/v1beta1"]),
        ],
    );
    let oracle = StaticOracle::new().with("networking.k8s.io/v1/Ingress", true);
    let candidates = missing_schema_attribution_candidates(&resource, &oracle);

    sim_assert_eq!(
        have: planned_api_versions(&candidates),
        want: vec!["networking.k8s.io/v1"]
    );
}

#[test]
fn missing_attribution_preserves_empty_primary_candidates_in_source_order() {
    let resource = resource(
        "",
        &["extensions/v1beta1", "networking.k8s.io/v1"],
        Vec::new(),
    );
    let candidates = missing_schema_attribution_candidates(&resource, &StaticOracle::new());

    sim_assert_eq!(
        have: planned_api_versions(&candidates),
        want: vec!["extensions/v1beta1", "networking.k8s.io/v1"]
    );
}

#[test]
fn missing_attribution_preserves_unresolved_branch_candidates_in_source_order() {
    let resource = resource(
        "",
        &["extensions/v1beta1", "networking.k8s.io/v1"],
        vec![branch_has("networking.k8s.io/v1/Ingress", &[])],
    );
    let oracle = StaticOracle::new().with("networking.k8s.io/v1/Ingress", true);
    let candidates = missing_schema_attribution_candidates(&resource, &oracle);

    sim_assert_eq!(
        have: planned_api_versions(&candidates),
        want: vec!["extensions/v1beta1", "networking.k8s.io/v1"]
    );
}

#[test]
fn missing_attribution_uses_primary_when_primary_is_present() {
    let resource = resource("extensions/v1beta1", &["networking.k8s.io/v1"], Vec::new());
    let candidates = missing_schema_attribution_candidates(&resource, &StaticOracle::new());

    sim_assert_eq!(
        have: planned_api_versions(&candidates),
        want: vec!["extensions/v1beta1"]
    );
}
