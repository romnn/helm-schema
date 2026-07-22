//! Semantic assertions for the istiod chart: `zzy_descope_legacy.yaml`
//! merges the `pilot` subtree IN PLACE over the values root
//! (`mustMergeOverwrite $.Values (index $.Values "pilot")`), so members
//! written under `pilot` overwrite their effective-root twins before any
//! template reads them. Root abort-grade contracts project back onto the
//! `pilot.*` spellings — `.Values.env.MCS_API_GROUP` aborts on a scalar
//! `env` however the user spells it. Values validation and the
//! full-schema pin live in `chart_corpus.rs`.

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
