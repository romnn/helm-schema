//! Pins the offline-safety contract for
//! `KubernetesJsonSchemaProvider::capability_has_query_at_primary_version`.
//!
//! When downloads are disabled, the oracle must NOT promote
//! "cache exists for this version but doesn't contain the probe target"
//! into a `Some(false)` answer. A partial cache with one unrelated file
//! does not prove the probe is absent upstream — only an authoritative
//! signal (negative-cache hit from a probe that already saw upstream
//! return "not found", or a successful online fetch attempt) does.
//! Without this contract, branch selection becomes a function of which
//! files happened to be fetched in previous runs.

use std::fs;
use std::sync::Arc;

use helm_schema_core::ApiPresenceQuery;
use helm_schema_k8s::{
    K8sSchemaProvider, K8sVersionChain, KubernetesJsonSchemaProvider, LookupTraceEntry,
    SourceProbeTraceOutcome, default_source_id,
};
use test_util::prelude::sim_assert_eq;

mod common;
use common::MockFetcher;

fn tmp_dir(label: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "helm-schema.{label}.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&p).expect("create temp dir");
    p
}

/// Offline mode plus a cache partially populated by an earlier run plus an
/// absent probe target must return `None` (uncertain), not `Some(false)`.
/// A cache with one unrelated file does not prove the rest of the bundle is
/// empty.
#[test]
fn offline_partial_cache_capability_probe_returns_none_when_target_absent() {
    let cache_dir = tmp_dir("capability_oracle_offline_partial");
    let primary = "v1.35.0";
    let source_id = default_source_id();

    // Plant an UNRELATED schema file. ConfigMap-v1 has nothing to do
    // with autoscaling/v2 / HorizontalPodAutoscaler — the presence of
    // this file proves only that *some* fetch happened for this K8s
    // version, not that the bundle is complete.
    let version_dir = cache_dir.join(source_id).join(primary);
    fs::create_dir_all(&version_dir).expect("create version dir");
    let unrelated = version_dir.join("configmap-v1.json");
    fs::write(&unrelated, r#"{"type":"object"}"#).expect("plant unrelated file");

    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec![primary.to_string()],
        None,
    ))
    .with_cache_dir(cache_dir.clone())
    .with_allow_download(false)
    .with_fetcher(Arc::new(MockFetcher::new()));

    let query =
        ApiPresenceQuery::parse_helm_literal("autoscaling/v2").expect("parse api presence query");
    let answer = provider.capability_has_query_at_primary_version(&query);
    sim_assert_eq!(
        have: answer,
        want: None,
        "offline + partial cache + probe target absent must return None (uncertain); \
         got {answer:?}. A partial cache does not prove the probe is absent upstream."
    );

    // Sanity-check: the planted ConfigMap IS discoverable as
    // `Has "v1"` (the canonical probe kind for the core v1 api
    // version is ConfigMap), so the cache layout itself is right.
    let query = ApiPresenceQuery::parse_helm_literal("v1").expect("parse api presence query");
    let positive = provider.capability_has_query_at_primary_version(&query);
    sim_assert_eq!(
        have: positive,
        want: Some(true),
        "the planted ConfigMap at v1 must be discoverable as Has(\"v1\"); got {positive:?}"
    );
}

#[test]
fn traced_offline_partial_cache_records_uncertain_source_probe() {
    let cache_dir = tmp_dir("capability_oracle_trace_partial");
    let primary = "v1.35.0";
    let source_id = default_source_id();
    let version_dir = cache_dir.join(source_id).join(primary);
    fs::create_dir_all(&version_dir).expect("create version dir");
    fs::write(
        version_dir.join("configmap-v1.json"),
        r#"{"type":"object"}"#,
    )
    .expect("plant unrelated file");

    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec![primary.to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(false)
    .with_fetcher(Arc::new(MockFetcher::new()));
    let query =
        ApiPresenceQuery::parse_helm_literal("autoscaling/v2").expect("parse api presence query");

    let traced = provider.capability_has_query_at_primary_version_traced(&query);

    sim_assert_eq!(have: traced.answer, want: None);
    assert!(
        traced.trace.entries().iter().any(|entry| matches!(
            entry,
            LookupTraceEntry::ApiPresenceSourceProbe {
                source_id,
                k8s_version,
                filename,
                outcome: SourceProbeTraceOutcome::Uncertain,
                ..
            } if source_id == default_source_id()
                && k8s_version == primary
                && filename == "horizontalpodautoscaler-autoscaling-v2.json"
        )),
        "offline partial cache must expose the uncertain HPA source probe in the trace: {:?}",
        traced.trace.entries()
    );
}

/// Offline + completely empty cache: the oracle has no information
/// either way and must abstain with `None`. A `Some(false)` here
/// would let a just-initialised cache silently misroute branch
/// selection.
#[test]
fn offline_empty_cache_returns_none() {
    let cache_dir = tmp_dir("capability_oracle_offline_empty");
    let primary = "v1.35.0";

    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec![primary.to_string()],
        None,
    ))
    .with_cache_dir(cache_dir.clone())
    .with_allow_download(false)
    .with_fetcher(Arc::new(MockFetcher::new()));

    let query =
        ApiPresenceQuery::parse_helm_literal("autoscaling/v2").expect("parse api presence query");
    let answer = provider.capability_has_query_at_primary_version(&query);
    sim_assert_eq!(
        have: answer,
        want: None,
        "offline + empty cache must return None; got {answer:?}"
    );
}

/// Online + the fetcher reports "not found": that's an authoritative
/// upstream signal. Return `Some(false)`. The negative cache also
/// gets populated so subsequent probes to the same target in the
/// same process short-circuit without re-fetching.
#[test]
fn online_404_returns_authoritative_false_and_caches_negative() {
    let cache_dir = tmp_dir("capability_oracle_online_404");
    let primary = "v1.35.0";

    let mock = Arc::new(MockFetcher::new()); // empty → every fetch is Ok(None) (404)
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec![primary.to_string()],
        None,
    ))
    .with_cache_dir(cache_dir.clone())
    .with_allow_download(true)
    .with_fetcher(mock.clone());

    // First probe: the fetcher is consulted, returns 404 → authoritative absent.
    let query =
        ApiPresenceQuery::parse_helm_literal("autoscaling/v2").expect("parse api presence query");
    let first = provider.capability_has_query_at_primary_version(&query);
    sim_assert_eq!(
        have: first,
        want: Some(false),
        "online + 404 must be authoritatively absent; got {first:?}"
    );

    // Second probe in the same process: negative cache should
    // short-circuit and still return Some(false) without re-fetching.
    // The fetcher's call count would prove no second request was
    // made, but the MockFetcher in this codebase doesn't expose that
    // — the contract here is "returns the same authoritative answer".
    let second = provider.capability_has_query_at_primary_version(&query);
    sim_assert_eq!(
        have: second,
        want: Some(false),
        "second probe in same process must still be authoritatively absent; got {second:?}"
    );
}

#[test]
fn traced_online_404_records_authoritative_absent_source_probe() {
    let cache_dir = tmp_dir("capability_oracle_trace_404");
    let primary = "v1.35.0";
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec![primary.to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(true)
    .with_fetcher(Arc::new(MockFetcher::new()));
    let query =
        ApiPresenceQuery::parse_helm_literal("autoscaling/v2").expect("parse api presence query");

    let traced = provider.capability_has_query_at_primary_version_traced(&query);

    sim_assert_eq!(have: traced.answer, want: Some(false));
    assert!(
        traced.trace.entries().iter().any(|entry| matches!(
            entry,
            LookupTraceEntry::ApiPresenceSourceProbe {
                filename,
                outcome: SourceProbeTraceOutcome::AuthoritativelyAbsent,
                ..
            } if filename == "horizontalpodautoscaler-autoscaling-v2.json"
        )),
        "online 404 must expose the authoritative absent source probe in the trace: {:?}",
        traced.trace.entries()
    );
}

/// Online + the fetcher returns a real schema document: the oracle
/// returns `Some(true)` and the cache is populated for subsequent
/// offline lookups.
#[test]
fn online_200_returns_true_and_caches() {
    let cache_dir = tmp_dir("capability_oracle_online_200");
    let primary = "v1.35.0";
    let source_id = default_source_id();
    // The default K8s mirror URL helm-schema uses (per K8sMirrorChain
    // default) — must match what the provider would compose for the
    // HPA-at-autoscaling/v2 probe.
    let default_url = format!(
        "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/{}/horizontalpodautoscaler-autoscaling-v2.json",
        primary
    );
    let mock = Arc::new(MockFetcher::new().with_body(
        default_url,
        br#"{"type":"object","properties":{}}"#.to_vec(),
    ));
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec![primary.to_string()],
        None,
    ))
    .with_cache_dir(cache_dir.clone())
    .with_allow_download(true)
    .with_fetcher(mock);

    let query =
        ApiPresenceQuery::parse_helm_literal("autoscaling/v2").expect("parse api presence query");
    let answer = provider.capability_has_query_at_primary_version(&query);
    sim_assert_eq!(
        have: answer,
        want: Some(true),
        "online + fetcher returns schema body must be Some(true); got {answer:?}"
    );

    // The fetch should have populated the disk cache so a follow-up
    // offline provider with the same cache_dir sees the file and
    // answers from the disk hit alone.
    let probe_file = cache_dir
        .join(source_id)
        .join(primary)
        .join("horizontalpodautoscaler-autoscaling-v2.json");
    assert!(
        probe_file.exists(),
        "successful online fetch must persist to disk for future offline reads"
    );
}
