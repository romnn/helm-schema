mod common;

#[test]
fn surveyor_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    common::assert_chart_values_yaml_validates("surveyor")?;
    Ok(())
}
