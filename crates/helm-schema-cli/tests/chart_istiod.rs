//! Semantic assertions for the istiod chart's effective-values rewrites.
//! `zzz_profile.yaml` rebuilds `.Values` from the hidden defaults source and
//! user-facing root, while `zzy_descope_legacy.yaml` merges `pilot` in place
//! over that effective root. The accepted input contract must therefore keep
//! both the pre-rewrite defaults source and the prefixed `pilot.*` spellings.
//! Values validation and the full-schema pin live in `chart_corpus.rs`.

use color_eyre::eyre;

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;
#[path = "common/values_yaml.rs"]
mod values_yaml;

#[test]
fn istiod_pilot_overlay_carries_root_contracts() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("istiod")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    for (label, override_, want) in [
        (
            "a scalar pilot.env aborts the member read",
            serde_json::json!({ "pilot": { "env": "oops" } }),
            false,
        ),
        (
            "a list pilot.env aborts the member read",
            serde_json::json!({ "pilot": { "env": [1] } }),
            false,
        ),
        (
            "a map pilot.env renders",
            serde_json::json!({ "pilot": { "env": { "MCS_API_GROUP": "custom.group" } } }),
            true,
        ),
        (
            "a scalar root env aborts the member read",
            serde_json::json!({ "env": "oops" }),
            false,
        ),
        (
            "a map root env renders",
            serde_json::json!({ "env": { "MCS_API_GROUP": "custom.group" } }),
            true,
        ),
        (
            "removing the effective-root defaults source aborts",
            serde_json::json!({ "_internal_defaults_do_not_set": null }),
            false,
        ),
    ] {
        let instance =
            chart_instances::with_override("istiod", override_).expect("compose instance");
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}"
        );
    }
    Ok(())
}
