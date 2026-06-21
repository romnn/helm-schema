#[path = "common/chart_validation.rs"]
mod chart_validation;

#[test]
fn zalando_postgres_operator_ui_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    chart_validation::assert_chart_values_yaml_validates("zalando-postgres-operator-ui")?;
    Ok(())
}
