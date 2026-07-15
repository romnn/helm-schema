//! Semantic assertions for the kyverno chart: the shared `kyverno.image`
//! helper explicitly fails on non-string image tags, and its chart-version
//! helper requires a version when global templating is enabled. The replicas
//! helper's zero-check does not decode and must not manufacture requirements.
//! Values validation and the full-schema pin live in `chart_corpus.rs`.

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn kyverno_image_tag_validator_holds() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("kyverno")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let numeric_tag = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "admissionController": { "container": { "image": { "tag": 7 } } }
        }),
    )?;
    assert!(
        !validator.is_valid(&numeric_tag),
        "the kyverno.image helper fails on non-string tags"
    );
    let string_tag = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "admissionController": { "container": { "image": { "tag": "v1.16.1" } } }
        }),
    )?;
    assert!(validator.is_valid(&string_tag), "string tags render");
    let replicas = chart_instances::with_override(
        "kyverno",
        serde_json::json!({ "backgroundController": { "replicas": 3 } }),
    )?;
    assert!(
        validator.is_valid(&replicas),
        "the replicas zero-check does not decode, so it must not reject normal counts"
    );
    Ok(())
}

#[test]
fn kyverno_templating_version_validator_survives_nested_helper_arguments()
-> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("kyverno")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let disabled = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "global": { "templating": { "enabled": false, "version": "" } }
        }),
    )?;
    assert!(
        validator.is_valid(&disabled),
        "disabled templating does not evaluate the required call"
    );
    let enabled = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "global": { "templating": { "enabled": true, "version": "1.16.1" } }
        }),
    )?;
    assert!(
        validator.is_valid(&enabled),
        "enabled templating accepts a nonempty version"
    );
    let empty_version = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "global": { "templating": { "enabled": true, "version": "" } }
        }),
    )?;
    assert!(
        !validator.is_valid(&empty_version),
        "the nested chartVersion helper rejects an empty version"
    );

    Ok(())
}
