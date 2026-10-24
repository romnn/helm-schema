//! Successful HTTP responses supersede earlier negative evidence before schema decoding.

use std::sync::Arc;

use color_eyre::eyre;
use helm_schema_core::ApiPresenceQuery;
use helm_schema_k8s::{K8sSchemaProvider as _, KubernetesJsonSchemaProvider, NegativeCache};
use serde_json::json;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

/// Shared provider fixtures for K8s integration tests.
pub mod common;
use common::{MockFetcher, MockResponse};

struct ExpectedRefresh {
    current: Option<bool>,
    offline: Option<bool>,
    negative: bool,
}

fn assert_refresh_evidence(response: MockResponse, expected: &ExpectedRefresh) -> eyre::Result<()> {
    let ExpectedRefresh {
        current,
        offline,
        negative: expected_negative,
    } = expected;
    let cache = ScratchDir::new("negative-cache-refresh")?;
    let negative = Arc::new(NegativeCache::new());
    let query = ApiPresenceQuery::GroupVersion {
        api_version: "v1".to_string(),
    };
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(cache.path())
        .with_allow_download(true)
        .with_negative_cache(Arc::clone(&negative))
        .with_fetcher(Arc::new(MockFetcher::new()));
    sim_assert_eq!(have: provider.capability_has_query_at_primary_version(&query), want: Some(false));

    negative.record("mirror", "v1.35.0", "configmap-v1.json");
    negative.record("default", "v1.29.0", "configmap-v1.json");
    negative.record("default", "v1.35.0", "secret-v1.json");

    // A forced refresh bypasses both earlier negative stores.
    let refresh = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(cache.path())
        .with_allow_download(true)
        .with_use_cache(false)
        .with_negative_cache(Arc::clone(&negative))
        .with_fetcher(Arc::new(MockFetcher::new().with_default(response)));
    let refreshed = refresh.capability_has_query_at_primary_version(&query);
    let marker = cache
        .path()
        .join("default/v1.35.0/configmap-v1.json.not-found");
    let offline_disk = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(cache.path())
        .with_allow_download(false);
    let offline_shared = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(cache.path())
        .with_allow_download(false)
        .with_negative_cache(Arc::clone(&negative));

    // Only a successful response contradicts absence, including an unusable body.
    // Other source, release, and filename slots keep their independent evidence.
    sim_assert_eq!(
        have: json!({
            "refreshed": refreshed,
            "disk_negative": marker.exists(),
            "shared_negative": negative.contains("default", "v1.35.0", "configmap-v1.json"),
            "offline_disk": offline_disk.capability_has_query_at_primary_version(&query),
            "offline_shared": offline_shared.capability_has_query_at_primary_version(&query),
            "other_source": negative.contains("mirror", "v1.35.0", "configmap-v1.json"),
            "other_release": negative.contains("default", "v1.29.0", "configmap-v1.json"),
            "other_filename": negative.contains("default", "v1.35.0", "secret-v1.json"),
        }),
        want: json!({
            "refreshed": current,
            "disk_negative": expected_negative,
            "shared_negative": expected_negative,
            "offline_disk": offline,
            "offline_shared": offline,
            "other_source": true,
            "other_release": true,
            "other_filename": true,
        })
    );
    Ok(())
}

#[test]
fn malformed_successful_response_supersedes_prior_absence() -> eyre::Result<()> {
    assert_refresh_evidence(
        MockResponse::Body(b"not JSON".to_vec()),
        &ExpectedRefresh {
            current: None,
            offline: None,
            negative: false,
        },
    )
}

#[test]
fn usable_successful_response_supersedes_prior_absence() -> eyre::Result<()> {
    assert_refresh_evidence(
        MockResponse::Body(br#"{"type":"object"}"#.to_vec()),
        &ExpectedRefresh {
            current: Some(true),
            offline: Some(true),
            negative: false,
        },
    )
}

#[test]
fn failed_refresh_preserves_prior_absence() -> eyre::Result<()> {
    assert_refresh_evidence(
        MockResponse::Transport("503 service unavailable".to_string()),
        &ExpectedRefresh {
            current: None,
            offline: Some(false),
            negative: true,
        },
    )
}
