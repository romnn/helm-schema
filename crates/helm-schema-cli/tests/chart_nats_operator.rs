#[path = "common/chart_validation.rs"]
mod chart_validation;

#[test]
fn nats_operator_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    chart_validation::assert_chart_values_yaml_validates("nats-operator")?;
    Ok(())
}
