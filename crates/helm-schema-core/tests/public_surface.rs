use helm_schema_core::{ApiPresenceQuery, ResourceRef, ordered_api_versions_for_resource};
use test_util::prelude::sim_assert_eq;

#[test]
fn api_presence_query_parses_resource_and_group_version_literals() {
    sim_assert_eq!(
        ApiPresenceQuery::parse_helm_literal("policy/v1/PodDisruptionBudget"),
        Some(ApiPresenceQuery::Resource {
            api_version: "policy/v1".to_string(),
            kind: "PodDisruptionBudget".to_string(),
        })
    );
    sim_assert_eq!(
        ApiPresenceQuery::parse_helm_literal("monitoring.coreos.com/v1"),
        Some(ApiPresenceQuery::GroupVersion {
            api_version: "monitoring.coreos.com/v1".to_string(),
        })
    );
}

#[test]
fn ordered_api_versions_prefers_stable_non_extensions_versions() {
    let resource = ResourceRef {
        api_version: "networking.k8s.io/v1beta1".to_string(),
        kind: "Ingress".to_string(),
        api_version_candidates: vec![
            "extensions/v1beta1".to_string(),
            "networking.k8s.io/v1".to_string(),
        ],
        api_version_branches: Vec::new(),
    };

    let versions = ordered_api_versions_for_resource(&resource);

    sim_assert_eq!(
        versions,
        vec![
            "networking.k8s.io/v1",
            "networking.k8s.io/v1beta1",
            "extensions/v1beta1"
        ]
    );
}
