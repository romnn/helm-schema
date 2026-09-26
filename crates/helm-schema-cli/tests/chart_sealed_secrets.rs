//! Semantic assertions for the sealed-secrets chart: the namespaced-roles
//! branch ranges `additionalNamespaces`, which iterates collections and
//! integer counts (Helm's `--set` channel delivers int64, which Go
//! templates range over) but fails on strings and non-integral numbers.
//! Values validation and the full-schema pin live in `chart_corpus.rs`.

use color_eyre::eyre;
use helm_schema_test_support::{ArtifactId, ChartId};

#[path = "common/chart_instances.rs"]
mod chart_instances;

#[test]
fn sealed_secrets_ranged_namespaces_domain_holds() -> eyre::Result<()> {
    let schema = helm_schema_test_support::consume(ArtifactId::Chart(ChartId::SealedSecrets))?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // Cases compose over the chart defaults: helm validates the coalesced
    // document, and the chart navigates hosts these overrides do not touch.
    let ranged = |value: serde_json::Value| {
        chart_instances::with_override(
            "sealed-secrets",
            serde_json::json!({
                "rbac": { "create": true, "namespacedRoles": true, "clusterRole": false },
                "additionalNamespaces": value
            }),
        )
        .expect("compose sealed-secrets instance")
    };
    for value in [
        serde_json::json!(["ns-a"]),
        serde_json::json!(2),
        serde_json::json!(0),
        serde_json::json!(-1),
        serde_json::json!(null),
    ] {
        assert!(
            validator.is_valid(&ranged(value.clone())),
            "range iterates {value} in the namespaced-roles branch"
        );
    }
    for value in [serde_json::json!("ns-a"), serde_json::json!(2.5)] {
        assert!(
            !validator.is_valid(&ranged(value.clone())),
            "range cannot iterate {value}"
        );
    }
    assert!(
        validator.is_valid(&chart_instances::with_override(
            "sealed-secrets",
            serde_json::json!({
                "rbac": { "namespacedRoles": false },
                "additionalNamespaces": "ns-a"
            })
        )?),
        "outside the ranged branch only join consumes the value"
    );
    Ok(())
}
