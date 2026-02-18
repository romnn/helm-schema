mod common;

#[test]
fn common_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    common::assert_chart_values_yaml_validates("common")?;
    Ok(())
}
