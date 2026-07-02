use std::fs;
use test_util::prelude::sim_assert_eq;

use referencing::uri;
use serde_json::json;

use super::*;

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "helm-schema-fetch-policy-{name}-{}",
        std::process::id()
    ))
}

#[test]
fn file_retrieval_respects_fetch_policy() {
    let path = temp_path("file");
    fs::write(&path, r#"{"type":"string"}"#).expect("write test schema");
    let canonical = path.canonicalize().expect("canonicalize test schema");
    let uri = Uri::parse(format!("file://{}", canonical.to_string_lossy())).expect("file uri");

    let denied = FsHttpRetrieve::new(FetchPolicy::new(false, false), LoadBudget::default())
        .retrieve(&uri)
        .expect_err("file retrieval should be denied");
    assert!(
        denied
            .to_string()
            .contains("local file access is disabled by fetch policy"),
        "unexpected denial error: {denied}"
    );

    let allowed = FsHttpRetrieve::new(FetchPolicy::new(true, false), LoadBudget::default())
        .retrieve(&uri)
        .expect("file retrieval should succeed");
    sim_assert_eq!(have: allowed, want: json!({ "type": "string" }));

    fs::remove_file(&path).expect("remove test schema");
}

#[test]
fn network_retrieval_respects_fetch_policy() {
    let uri = uri::from_str("https://example.com/schema.json").expect("https uri");
    let err = FsHttpRetrieve::new(FetchPolicy::new(true, false), LoadBudget::default())
        .retrieve(&uri)
        .expect_err("network retrieval should be denied");
    assert!(
        err.to_string()
            .contains("network access is disabled by fetch policy"),
        "unexpected denial error: {err}"
    );
}

#[test]
fn file_retrieval_rejects_non_empty_file_authority_host() {
    let uri = uri::from_str("file://localhost/tmp/schema.json").expect("file uri");
    let err = FsHttpRetrieve::new(FetchPolicy::new(true, false), LoadBudget::default())
        .retrieve(&uri)
        .expect_err("file retrieval should reject authority host");
    assert!(
        err.to_string()
            .contains("file:// authority host is not allowed by fetch policy"),
        "unexpected file-host error: {err}"
    );
}

#[test]
fn network_retrieval_rejects_loopback_and_link_local_hosts() {
    for uri_text in [
        "http://127.0.0.1/schema.json",
        "http://[::1]/schema.json",
        "http://localhost/schema.json",
        "http://169.254.169.254/latest/meta-data",
    ] {
        let uri = uri::from_str(uri_text).expect("network uri");
        let err = FsHttpRetrieve::new(FetchPolicy::input_assembly(true), LoadBudget::default())
            .retrieve(&uri)
            .expect_err("unsafe host should be rejected before network access");
        assert!(
            err.to_string().contains("denied by fetch policy"),
            "unexpected network-host error for {uri_text}: {err}"
        );
    }
}

#[test]
fn file_retrieval_respects_load_budget() {
    let path = temp_path("file-budget");
    fs::write(&path, r#"{"type":"string"}"#).expect("write test schema");
    let canonical = path.canonicalize().expect("canonicalize test schema");
    let uri = Uri::parse(format!("file://{}", canonical.to_string_lossy())).expect("file uri");

    let err = FsHttpRetrieve::new(FetchPolicy::new(true, false), LoadBudget::new(64, 4))
        .retrieve(&uri)
        .expect_err("file retrieval should exceed budget");
    assert!(
        err.to_string().contains("load budget exceeded"),
        "unexpected budget error: {err}"
    );

    fs::remove_file(&path).expect("remove test schema");
}
