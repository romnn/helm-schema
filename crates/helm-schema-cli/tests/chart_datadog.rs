//! Semantic assertions for the datadog chart: `agents.image.tag` flows
//! through `toString | trimSuffix "-jmx"` pipelines (conditions and local
//! assignments), where the conversion runs BEFORE the string consumer, so
//! any input type renders. Values validation and the full-schema pin live
//! in `chart_corpus.rs`.

#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn datadog_image_tag_accepts_non_strings() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("datadog")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    for tag in [serde_json::json!(7), serde_json::json!("7.68.2")] {
        assert!(
            validator.is_valid(&serde_json::json!({
                "agents": { "image": { "tag": tag, "doNotCheckTag": true } }
            })),
            "toString converts the tag before trimSuffix consumes it: tag={tag}"
        );
    }
    Ok(())
}
