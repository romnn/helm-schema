mod common;

#[test]
fn bitnami_redis_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    common::assert_chart_values_yaml_validates("bitnami-redis")?;
    Ok(())
}
