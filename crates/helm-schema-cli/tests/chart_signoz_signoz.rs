mod common;

#[test]
fn signoz_signoz_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    common::assert_chart_values_yaml_validates("signoz-signoz")?;
    Ok(())
}
