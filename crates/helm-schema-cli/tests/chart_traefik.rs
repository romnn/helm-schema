//! Semantic assertions for the traefik chart: the pod template ranges
//! `experimental.plugins` and explicitly fails unless each value is an
//! object carrying both `moduleName` and `version`, so those are
//! per-member validator requirements. Values validation and the
//! full-schema pin live in `chart_corpus.rs`.

#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn traefik_plugin_validator_holds() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("traefik")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let plugins =
        |value: serde_json::Value| serde_json::json!({ "experimental": { "plugins": value } });
    for bad in [
        serde_json::json!({ "bad": 7 }),
        serde_json::json!({ "bad": { "moduleName": "x" } }),
        serde_json::json!({ "bad": { "version": "v1" } }),
    ] {
        assert!(
            !validator.is_valid(&plugins(bad.clone())),
            "plugins without moduleName+version objects fail rendering: {bad}"
        );
    }
    assert!(
        validator.is_valid(&plugins(serde_json::json!({
            "ok": { "moduleName": "github.com/x/y", "version": "v1.0.0" }
        }))),
        "a complete plugin renders"
    );
    assert!(
        validator.is_valid(&plugins(serde_json::json!({}))),
        "the declared empty map stays valid"
    );
    Ok(())
}
