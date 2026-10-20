//! Definitions metadata cannot replace an authoritative resource probe.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::ApiPresenceQuery;
use helm_schema_k8s::{K8sSchemaProvider, KubernetesJsonSchemaProvider};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

/// Deterministic provider transport.
pub mod common;
use common::MockFetcher;

const UPSTREAM: &str = "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master";

// Frozen pre-index anchors: changing production rows must not change the baseline.
const OLD_ANCHORS: &[(&str, &str)] = &[
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

fn bundle(version: &str) -> eyre::Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(
        test_util::workspace_testdata()
            .join("provider-bundle/kubernetes-json-schema-cache/default")
            .join(version)
            .join("_definitions.json"),
    )?)?)
}

fn gvks(document: &Value) -> eyre::Result<BTreeSet<(String, String)>> {
    let mut result = BTreeSet::new();
    for definition in document["definitions"]
        .as_object()
        .ok_or_eyre("missing definitions")?
        .values()
    {
        let Some(entries) = definition["x-kubernetes-group-version-kind"].as_array() else {
            continue;
        };
        for entry in entries {
            let group = entry["group"].as_str().ok_or_eyre("missing group")?;
            let version = entry["version"].as_str().ok_or_eyre("missing version")?;
            let kind = entry["kind"].as_str().ok_or_eyre("missing kind")?;
            let api_version = if group.is_empty() {
                version.to_string()
            } else {
                format!("{group}/{version}")
            };
            result.insert((api_version, kind.to_string()));
        }
    }
    Ok(result)
}

#[test]
fn all_old_anchors_match_across_four_pinned_releases() -> eyre::Result<()> {
    sim_assert_eq!(have: OLD_ANCHORS.len(), want: 41);
    let fixes = [
        ("v1.15.0", vec!["networking.k8s.io/v1"]),
        ("v1.24.0", vec!["storage.k8s.io/v1beta1"]),
        ("v1.29.0", vec!["admissionregistration.k8s.io/v1beta1"]),
        (
            "v1.35.0",
            vec![
                "admissionregistration.k8s.io/v1beta1",
                "certificates.k8s.io/v1beta1",
                "coordination.k8s.io/v1beta1",
                "networking.k8s.io/v1beta1",
                "storage.k8s.io/v1beta1",
            ],
        ),
    ];
    for ((release, allowed_fixes), regression_count) in fixes.into_iter().zip([0, 11, 19, 16]) {
        let document = bundle(release)?;
        let inventory = gvks(&document)?;
        let cache = ScratchDir::new("capability-anchor-comparison")?;
        let directory = cache.path().join("default").join(release);
        std::fs::create_dir_all(&directory)?;
        std::fs::write(
            directory.join("_definitions.json"),
            serde_json::to_vec(&document)?,
        )?;
        let mut fetcher = MockFetcher::new();
        let mut expected = BTreeMap::new();
        let mut generic_only = BTreeSet::new();
        for &(api_version, kind) in OLD_ANCHORS {
            let present = inventory.contains(&(api_version.to_string(), kind.to_string()));
            expected.insert(api_version.to_string(), Some(present));
            if present {
                let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
                let filename = if group.is_empty() {
                    format!("{}-{version}.json", kind.to_ascii_lowercase())
                } else {
                    let prefix = group.split('.').next().ok_or_eyre("missing group prefix")?;
                    format!("{}-{prefix}-{version}.json", kind.to_ascii_lowercase())
                };
                fetcher =
                    fetcher.with_body(format!("{UPSTREAM}/{release}/{filename}"), b"{}".to_vec());
            } else if inventory
                .iter()
                .any(|(candidate, _)| candidate == api_version)
            {
                generic_only.insert(api_version);
            }
        }
        let expected_fixes = allowed_fixes.into_iter().collect::<BTreeSet<_>>();
        sim_assert_eq!(have: generic_only.intersection(&expected_fixes).count(), want: expected_fixes.len());
        sim_assert_eq!(have: generic_only.difference(&expected_fixes).count(), want: regression_count);
        let fetcher = Arc::new(fetcher);
        let transport = Arc::clone(&fetcher);
        let cold = KubernetesJsonSchemaProvider::new(release)
            .with_cache_dir(cache.path())
            .with_allow_download(true)
            .with_fetcher(transport);
        let mut actual = BTreeMap::new();
        for &(api_version, _) in OLD_ANCHORS {
            let query =
                ApiPresenceQuery::parse_helm_literal(api_version).ok_or_eyre("invalid anchor")?;
            actual.insert(
                api_version.to_string(),
                cold.capability_has_query_at_primary_version(&query),
            );
        }
        sim_assert_eq!(have: actual, want: expected, "release {release}");
        let warm = KubernetesJsonSchemaProvider::new(release)
            .with_cache_dir(cache.path())
            .with_allow_download(false);
        for &(api_version, _) in OLD_ANCHORS {
            let query =
                ApiPresenceQuery::parse_helm_literal(api_version).ok_or_eyre("invalid anchor")?;
            sim_assert_eq!(have: warm.capability_has_query_at_primary_version(&query), want: expected[api_version], "warm {release} {api_version}");
        }
        sim_assert_eq!(have: fetcher.calls_for(&format!("{UPSTREAM}/{release}/_definitions.json")), want: 0);
    }
    Ok(())
}

fn collect_references(value: &Value, references: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|reference| reference.strip_prefix("#/definitions/"))
            {
                references.insert(reference.to_string());
            }
            for child in object.values() {
                collect_references(child, references);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_references(child, references);
            }
        }
        _ => {}
    }
}

// Retaining every singleton core GVK and closing every reference still omits policy resources.
fn core_only_index(document: &Value) -> eyre::Result<Value> {
    let definitions = document["definitions"]
        .as_object()
        .ok_or_eyre("missing definitions")?;
    let mut selected = BTreeSet::new();
    for (name, definition) in definitions {
        let Some(entries) = definition["x-kubernetes-group-version-kind"].as_array() else {
            continue;
        };
        if entries.len() == 1
            && entries
                .iter()
                .all(|entry| entry["group"] == "" && entry["version"] == "v1")
        {
            selected.insert(name.clone());
        }
    }
    loop {
        let mut references = selected.clone();
        for name in &selected {
            collect_references(&definitions[name], &mut references);
        }
        if references == selected {
            break;
        }
        selected = references;
    }
    let mut retained = serde_json::Map::new();
    for name in selected {
        retained.insert(
            name.clone(),
            definitions
                .get(&name)
                .ok_or_eyre("dangling reference")?
                .clone(),
        );
    }
    Ok(json!({"definitions": retained}))
}

#[test]
fn partial_indexes_cannot_prove_absence_online_or_offline() -> eyre::Result<()> {
    let release = "v1.29.0";
    let full = bundle(release)?;
    let config_map = full["definitions"]["io.k8s.api.core.v1.ConfigMap"].clone();
    let partials = [
        json!({"definitions": {"io.k8s.api.core.v1.ConfigMap": config_map}}),
        core_only_index(&full)?,
    ];
    let query = ApiPresenceQuery::parse_helm_literal("policy/v1").ok_or_eyre("invalid query")?;
    for document in partials {
        assert!(
            !gvks(&document)?
                .contains(&("policy/v1".to_string(), "PodDisruptionBudget".to_string()))
        );
        let cache = ScratchDir::new("capability-partial-index")?;
        let directory = cache.path().join("default").join(release);
        std::fs::create_dir_all(&directory)?;
        std::fs::write(
            directory.join("_definitions.json"),
            serde_json::to_vec(&document)?,
        )?;
        let offline = KubernetesJsonSchemaProvider::new(release)
            .with_cache_dir(cache.path())
            .with_allow_download(false);
        sim_assert_eq!(have: offline.capability_has_query_at_primary_version(&query), want: None);
        let fetcher = Arc::new(MockFetcher::new().with_body(
            format!("{UPSTREAM}/{release}/poddisruptionbudget-policy-v1.json"),
            b"{}".to_vec(),
        ));
        let transport = Arc::clone(&fetcher);
        let online = KubernetesJsonSchemaProvider::new(release)
            .with_cache_dir(cache.path())
            .with_allow_download(true)
            .with_fetcher(transport);
        sim_assert_eq!(have: online.capability_has_query_at_primary_version(&query), want: Some(true));
        let cold_cache = ScratchDir::new("capability-empty-index")?;
        let transport = Arc::clone(&fetcher);
        let cold = KubernetesJsonSchemaProvider::new(release)
            .with_cache_dir(cold_cache.path())
            .with_allow_download(true)
            .with_fetcher(transport);
        sim_assert_eq!(have: cold.capability_has_query_at_primary_version(&query), want: Some(true));
        let warm = KubernetesJsonSchemaProvider::new(release)
            .with_cache_dir(cache.path())
            .with_allow_download(false);
        sim_assert_eq!(have: warm.capability_has_query_at_primary_version(&query), want: Some(true));
        sim_assert_eq!(have: fetcher.calls_for(&format!("{UPSTREAM}/{release}/_definitions.json")), want: 0);
    }
    Ok(())
}
