//! Semantic assertions for bitnami-redis: values-comment descriptions must
//! land on the right schema nodes. Values validation and the full-schema pin
//! live in `chart_corpus.rs`.

use test_util::prelude::sim_assert_eq;
#[path = "common/descriptions.rs"]
mod descriptions;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn bitnami_redis_values_descriptions_apply() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("bitnami-redis")?;
    assert_schema_description(
        &schema,
        "/properties/auth/properties/enabled/description",
        "Enable password authentication",
    );
    assert_schema_description(
        &schema,
        "/properties/image/properties/registry/description",
        "[default: REGISTRY_NAME] Redis(R) image registry",
    );
    assert_schema_description(
        &schema,
        "/properties/global/properties/imageRegistry/description",
        "Global Docker image registry",
    );
    descriptions::assert_chart_values_comments_apply_to_existing_schema_paths(
        "bitnami-redis",
        &schema,
        50,
    )?;

    Ok(())
}

fn assert_schema_description(schema: &serde_json::Value, pointer: &str, expected: &str) {
    sim_assert_eq!(
        have: schema.pointer(pointer).and_then(serde_json::Value::as_str),
        want: Some(expected),
        "schema description mismatch at {pointer}"
    );
}
