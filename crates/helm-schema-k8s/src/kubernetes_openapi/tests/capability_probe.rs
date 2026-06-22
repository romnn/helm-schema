use super::*;
use test_util::prelude::sim_assert_eq;

fn probe(api: &str) -> Option<ResourceRef> {
    let query = ApiPresenceQuery::parse_helm_literal(api)?;
    DEFAULT_CAPABILITY_PROBE_TABLE.build_probe(&query)
}

#[test]
fn group_version_probe_uses_canonical_kind_table() {
    let probe = probe("policy/v1").expect("policy/v1 should have a canonical probe");

    sim_assert_eq!(have: probe.api_version, want: "policy/v1");
    sim_assert_eq!(have: probe.kind, want: "PodDisruptionBudget");
}

#[test]
fn core_version_probe_uses_canonical_kind_table() {
    let probe = probe("v1").expect("core v1 should have a canonical probe");

    sim_assert_eq!(have: probe.api_version, want: "v1");
    sim_assert_eq!(have: probe.kind, want: "ConfigMap");
}

#[test]
fn resource_qualified_probe_bypasses_canonical_kind_table() {
    let probe = probe("policy/v1/PodSecurityPolicy").expect("resource probe should be direct");

    sim_assert_eq!(have: probe.api_version, want: "policy/v1");
    sim_assert_eq!(have: probe.kind, want: "PodSecurityPolicy");
}

#[test]
fn core_resource_qualified_probe_bypasses_canonical_kind_table() {
    let probe = probe("v1/Secret").expect("core resource probe should be direct");

    sim_assert_eq!(have: probe.api_version, want: "v1");
    sim_assert_eq!(have: probe.kind, want: "Secret");
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
