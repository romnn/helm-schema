//! Semantic assertions for Argo CD's ranged cluster-credential validators.
//!
//! The full-schema fixture and default-values validation live in
//! `chart_corpus.rs`.

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn argo_cd_cluster_credentials_require_config_per_entry() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("argo-cd")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let valid = chart_instances::with_override(
        "argo-cd",
        serde_json::json!({
            "configs": {
                "clusterCredentials": {
                    "prod": {
                        "server": "https://example.com",
                        "config": { "bearerToken": "token" }
                    }
                }
            }
        }),
    )?;
    assert!(
        validator.is_valid(&valid),
        "a complete cluster credential renders"
    );
    let invalid = chart_instances::with_override(
        "argo-cd",
        serde_json::json!({
            "configs": {
                "clusterCredentials": {
                    "prod": { "server": "https://example.com" }
                }
            }
        }),
    )?;
    assert!(
        !validator.is_valid(&invalid),
        "the required call inside stringData.config rejects an incomplete entry"
    );

    Ok(())
}
