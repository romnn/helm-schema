//! Semantic assertions for the sealed-secrets chart: the namespaced-roles
//! branch ranges `additionalNamespaces`, which iterates collections and
//! integer counts (Helm's `--set` channel delivers int64, which Go
//! templates range over) but fails on strings and non-integral numbers.
//! Values validation and the full-schema pin live in `chart_corpus.rs`.

#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn sealed_secrets_ranged_namespaces_domain_holds() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("sealed-secrets")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // The coalesced document carries the declared `rbac.create: true`; a
    // missing key was null-deleted and keeps the whole branch dormant.
    let ranged = |value: serde_json::Value| {
        serde_json::json!({
            "rbac": { "create": true, "namespacedRoles": true, "clusterRole": false },
            "additionalNamespaces": value
        })
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
        validator.is_valid(&serde_json::json!({
            "rbac": { "namespacedRoles": false },
            "additionalNamespaces": "ns-a"
        })),
        "outside the ranged branch only join consumes the value"
    );
    Ok(())
}
