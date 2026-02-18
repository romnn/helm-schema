mod common;

#[test]
fn zalando_postgres_operator_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    common::assert_chart_values_yaml_validates("zalando-postgres-operator")?;
    Ok(())
}
