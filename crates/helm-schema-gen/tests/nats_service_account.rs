#![recursion_limit = "512"]

mod common;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/nats/templates/service-account.yaml",
    values_path: "charts/nats/values.yaml",
    expected_fixture: include_str!("fixtures/nats_service_account.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &[
            "charts/nats/templates/_helpers.tpl",
            "charts/nats/templates/_jsonpatch.tpl",
            "charts/nats/templates/_tplYaml.tpl",
            "charts/nats/templates/_toPrettyRawJson.tpl",
        ],
        helper_template_dirs: &[],
        file_sources: &[(
            "files/service-account.yaml",
            "charts/nats/files/service-account.yaml",
        )],
    },
    provider: common::ProviderKind::K8s("v1.35.0"),
    dump_stem: "nats-service-account",
};

#[test]
fn schema_from_tree_sitter() {
    common::assert_schema_fixture(&CASE);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/nats");
    let rendered = common::helm_template_render_with_args(
        &chart_dir,
        Some("templates/service-account.yaml"),
        &["--set", "serviceAccount.enabled=true"],
    );

    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn schema_validates_values_yaml() {
    common::assert_values_yaml_validates(&CASE);
}

#[test]
fn schema_keeps_live_service_account_name_typed() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAccount": {
                    "enabled": true,
                    "name": 7
                }
            })
        ),
        "serviceAccount.name must stay string-like when ServiceAccount rendering is enabled: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAccount": {
                    "enabled": false,
                    "name": 7
                }
            })
        ),
        "serviceAccount.name should remain unconstrained when the ServiceAccount is disabled: {schema}"
    );
}
