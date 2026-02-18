mod common;

#[test]
fn nats_kafka_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    common::assert_chart_values_yaml_validates("nats-kafka")?;
    Ok(())
}
