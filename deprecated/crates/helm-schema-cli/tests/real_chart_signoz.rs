use assert_cmd::Command;
use serde_json::Value;
use std::path::PathBuf;
use tempfile::TempDir;

fn allow_net() -> bool {
    std::env::var("HELM_SCHEMA_ALLOW_NET")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn has_any_ref(v: &Value) -> bool {
    match v {
        Value::Object(o) => {
            if o.contains_key("$ref") {
                return true;
            }
            o.values().any(has_any_ref)
        }
        Value::Array(a) => a.iter().any(has_any_ref),
        _ => false,
    }
}

#[test]
fn signoz_chart_generates_self_contained_values_schema() -> Result<(), Box<dyn std::error::Error>> {
    if !allow_net() {
        // Real-chart integration tests require network.
        return Ok(());
    }

    // Pinned chart ref.
    let chart_ref = "signoz/signoz";
    let repo_url = "https://charts.signoz.io";
    let version = "0.105.2";

    // Use a temp cache for kubernetes-json-schema so the test is hermetic.
    let td = TempDir::new()?;
    let cache_dir = td.path().join("kubernetes-json-schema");

    let mut cmd = Command::cargo_bin("helm-schema")?;
    let out_bytes = cmd
        .arg("values-schema")
        .arg(chart_ref)
        .arg("--repo-url")
        .arg(repo_url)
        .arg("--version")
        .arg(version)
        .arg("--k8s-schema-cache")
        .arg(&cache_dir)
        .arg("--k8s-version")
        .arg("v1.29.0-standalone-strict")
        .arg("--allow-net")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let schema: Value = serde_json::from_slice(&out_bytes)?;

    assert_eq!(
        schema.get("$schema").and_then(|v| v.as_str()),
        Some("http://json-schema.org/draft-07/schema#")
    );
    assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));

    let props_nonempty = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .is_some_and(|m| !m.is_empty());
    assert!(
        props_nonempty,
        "expected non-empty properties at schema root"
    );

    assert!(
        !has_any_ref(&schema),
        "values schema must be self-contained (no $ref anywhere)"
    );

    // Keep a few lightweight sanity checks stable.
    let _ = PathBuf::from(cache_dir);
    Ok(())
}
