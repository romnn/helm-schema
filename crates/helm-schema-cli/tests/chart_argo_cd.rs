//! Semantic assertions for Argo CD's ranged cluster-credential validators.
//!
//! The full-schema fixture and default-values validation live in
//! `chart_corpus.rs`.

use color_eyre::eyre;
use helm_schema_test_support::{ArtifactId, ChartId};

#[path = "common/chart_instances.rs"]
mod chart_instances;

#[test]
fn argo_cd_cluster_credentials_require_config_per_entry() -> eyre::Result<()> {
    let schema = helm_schema_test_support::consume(ArtifactId::Chart(ChartId::ArgoCd))?;
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
