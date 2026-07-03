use super::{first_mapping_colon_offset, parse_yaml_key};
use test_util::prelude::sim_assert_eq;

#[test]
fn parse_yaml_key_handles_plain_and_quoted_keys() {
    sim_assert_eq!(
        have: parse_yaml_key("metadata.name: value"),
        want: Some("metadata.name".to_string())
    );
    sim_assert_eq!(
        have: parse_yaml_key(r#""app.kubernetes.io/name": value"#),
        want: Some("app.kubernetes.io/name".to_string())
    );
    sim_assert_eq!(
        have: parse_yaml_key("'it''s': value"),
        want: Some("it's".to_string())
    );
}

#[test]
fn first_mapping_colon_skips_templates_and_quoted_scalars() {
    let line = r#"{{ printf "not:a:key" }}: value"#;
    sim_assert_eq!(have: first_mapping_colon_offset(line), want: Some(24));

    let line = r#""not:a:key": value"#;
    sim_assert_eq!(have: first_mapping_colon_offset(line), want: Some(11));
}
