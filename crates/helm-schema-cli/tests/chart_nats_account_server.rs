mod common;

#[test]
fn nats_account_server_values_yaml_validates() -> color_eyre::eyre::Result<()> {
    common::assert_chart_values_yaml_validates("nats-account-server")?;
    Ok(())
}
