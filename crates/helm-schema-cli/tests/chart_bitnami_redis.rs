use test_util::prelude::sim_assert_eq;
mod common;

#[test]
fn bitnami_redis_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    let schema = common::generate_chart_schema("bitnami-redis")?;
    let values_json = common::values_yaml_as_json("bitnami-redis")?;
    common::assert_values_json_validates(&values_json, &schema);

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
    common::assert_chart_values_comments_apply_to_existing_schema_paths(
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
