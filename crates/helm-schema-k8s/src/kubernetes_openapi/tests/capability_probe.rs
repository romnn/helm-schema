use super::*;
use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use std::collections::BTreeSet;
use test_util::prelude::sim_assert_eq;

fn probe(api: &str) -> Option<ResourceRef> {
    let query = ApiPresenceQuery::parse_helm_literal(api)?;
    build_capability_probe(&query)
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

/// Uses releases spanning API deprecation windows so every retained row has a real document.
#[test]
fn capability_probe_rows_exist_in_pinned_provider_corpus() -> eyre::Result<()> {
    let root = test_util::workspace_testdata()
        .join("provider-bundle/kubernetes-json-schema-cache/default");
    let mut resources = BTreeSet::new();
    for version in ["v1.15.0", "v1.24.0", "v1.29.0", "v1.35.0"] {
        let path = root.join(version).join("_definitions.json");
        let bytes = std::fs::read(&path)
            .wrap_err_with(|| format!("read provider corpus {}", path.display()))?;
        let document: serde_json::Value = serde_json::from_slice(&bytes)
            .wrap_err_with(|| format!("parse provider corpus {}", path.display()))?;
        let definitions = document
            .get("definitions")
            .and_then(serde_json::Value::as_object)
            .ok_or_eyre("provider corpus has no definitions object")?;
        for definition in definitions.values() {
            let Some(gvks) = definition
                .get("x-kubernetes-group-version-kind")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            for gvk in gvks {
                let Some(group) = gvk.get("group").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let Some(version) = gvk.get("version").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let Some(kind) = gvk.get("kind").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let api_version = if group.is_empty() {
                    version.to_string()
                } else {
                    format!("{group}/{version}")
                };
                resources.insert((api_version, kind.to_string()));
            }
        }
    }

    let missing = WELL_KNOWN_API_VERSION_PROBES
        .iter()
        .filter(|(api_version, kind)| {
            !resources.contains(&(api_version.to_string(), kind.to_string()))
        })
        .copied()
        .collect::<Vec<_>>();

    sim_assert_eq!(have: missing, want: Vec::<(&str, &str)>::new());
    Ok(())
}
