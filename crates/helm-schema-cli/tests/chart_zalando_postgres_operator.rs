#[path = "common/chart_validation.rs"]
mod chart_validation;

#[test]
fn zalando_postgres_operator_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    chart_validation::assert_chart_values_yaml_validates("zalando-postgres-operator")?;
    Ok(())
}
