#[path = "common/chart_validation.rs"]
mod chart_validation;

#[test]
fn surveyor_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    chart_validation::assert_chart_values_yaml_validates("surveyor")?;
    Ok(())
}
