mod common;

#[test]
fn cert_manager_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    common::assert_chart_values_yaml_validates("cert-manager")?;
    Ok(())
}
