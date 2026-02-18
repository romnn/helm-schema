mod common;

#[test]
fn nack_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    common::assert_chart_values_yaml_validates("nack")?;
    Ok(())
}
