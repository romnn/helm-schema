//! Semantic assertions for the kyverno chart: the shared `kyverno.image`
//! helper explicitly fails on non-string image tags, so tags are validator
//! requirements; the replicas helper's zero-check does not decode and must
//! NOT manufacture requirements. Values validation and the full-schema pin
//! live in `chart_corpus.rs`.

#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn kyverno_image_tag_validator_holds() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("kyverno")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    assert!(
        !validator.is_valid(&serde_json::json!({
            "admissionController": { "container": { "image": { "tag": 7 } } }
        })),
        "the kyverno.image helper fails on non-string tags"
    );
    assert!(
        validator.is_valid(&serde_json::json!({
            "admissionController": { "container": { "image": { "tag": "v1.16.1" } } }
        })),
        "string tags render"
    );
    assert!(
        validator.is_valid(&serde_json::json!({
            "backgroundController": { "replicas": 3 }
        })),
        "the replicas zero-check does not decode, so it must not reject normal counts"
    );
    Ok(())
}
