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

#[cfg(unix)]
#[test]
fn failed_marker_deletion_and_schema_write_do_not_restore_absence() -> eyre::Result<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;

    let cache = ScratchDir::new("negative-cache-read-only-refresh")?;
    let negative = Arc::new(NegativeCache::new());
    let provider = || {
        KubernetesJsonSchemaProvider::new("v1.35.0")
            .with_cache_dir(cache.path())
            .with_allow_download(true)
            .with_negative_cache(Arc::clone(&negative))
    };
    let query = ApiPresenceQuery::GroupVersion {
        api_version: "v1".to_string(),
    };
    let initial = provider().with_fetcher(Arc::new(MockFetcher::new()));
    sim_assert_eq!(have: initial.capability_has_query_at_primary_version(&query), want: Some(false));

    // A readable, unwritable directory keeps the old marker but prevents both
    // its deletion and persistence of the successful response's valid schema.
    let directory = cache.path().join("default/v1.35.0");
    let original_permissions = fs::metadata(&directory)?.permissions();
    let mut read_only = original_permissions.clone();
    read_only.set_mode(read_only.mode() & !0o222);
    fs::set_permissions(&directory, read_only)?;
    let refresh = provider().with_use_cache(false).with_fetcher(Arc::new(
        MockFetcher::new().with_default(MockResponse::Body(br#"{"type":"object"}"#.to_vec())),
    ));
    let refreshed = refresh.capability_has_query_at_primary_version(&query);
    fs::set_permissions(&directory, original_permissions)?;

    // A new provider sharing the evidence must ignore the surviving marker.
    let offline = provider().with_allow_download(false);
    sim_assert_eq!(
        have: json!({
            "refreshed": refreshed,
            "disk_negative": directory.join("configmap-v1.json.not-found").is_file(),
            "schema_persisted": directory.join("configmap-v1.json").exists(),
            "shared_negative": negative.contains("default", "v1.35.0", "configmap-v1.json"),
            "offline": offline.capability_has_query_at_primary_version(&query),
        }),
        want: json!({
            "refreshed": null,
            "disk_negative": true,
            "schema_persisted": false,
            "shared_negative": false,
            "offline": null,
        })
    );

    // Cache-enabled online probes retry contradicted markers, and a fresh 404
    // establishes authoritative absence again.
    let retry_fetcher = Arc::new(MockFetcher::new());
    let retry = provider().with_fetcher(retry_fetcher.clone());
    sim_assert_eq!(have: retry.capability_has_query_at_primary_version(&query), want: Some(false));
    sim_assert_eq!(have: retry_fetcher.total_calls(), want: 1);
    sim_assert_eq!(have: offline.capability_has_query_at_primary_version(&query), want: Some(false));
    Ok(())
}
