use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

use referencing::uri;
use serde_json::json;
use url::Url;

use super::*;

#[test]
fn file_retrieval_respects_fetch_policy() -> eyre::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let directory = temp_dir.path().join("directory with #");
    std::fs::create_dir(&directory)?;
    let path = directory.join("schema.json");
    std::fs::write(&path, r#"{"type":"string"}"#)?;
    let base_uri = Url::parse(&directory_file_uri(&directory)?)?;
    let uri = uri::from_str(base_uri.join("schema.json")?.as_str())?;

    let denied = FsHttpRetrieve::new(FetchPolicy::new(false, false), LoadBudget::default())
        .retrieve(&uri)
        .err()
        .ok_or_eyre("file retrieval should be denied")?;
    assert!(
        denied
            .to_string()
            .contains("local file access is disabled by fetch policy"),
        "unexpected denial error: {denied}"
    );

    let allowed = FsHttpRetrieve::new(FetchPolicy::new(true, false), LoadBudget::default())
        .retrieve(&uri)
        .map_err(|error| eyre::Report::msg(error.to_string()))?;
    sim_assert_eq!(have: allowed, want: json!({ "type": "string" }));

    Ok(())
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
fn file_retrieval_respects_load_budget() -> eyre::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let directory = temp_dir.path().join("file-budget directory with #");
    std::fs::create_dir(&directory)?;
    let path = directory.join("schema.json");
    std::fs::write(&path, r#"{"type":"string"}"#)?;
    let base_uri = Url::parse(&directory_file_uri(&directory)?)?;
    let uri = uri::from_str(base_uri.join("schema.json")?.as_str())?;

    let err = FsHttpRetrieve::new(FetchPolicy::new(true, false), LoadBudget::new(64, 4))
        .retrieve(&uri)
        .err()
        .ok_or_eyre("file retrieval should exceed budget")?;
    assert!(
        err.to_string().contains("load budget exceeded"),
        "unexpected budget error: {err}"
    );

    Ok(())
}
