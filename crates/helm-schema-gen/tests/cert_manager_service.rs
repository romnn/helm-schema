#![recursion_limit = "512"]

mod common;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/cert-manager/templates/service.yaml",
    values_path: "charts/cert-manager/values.yaml",
    expected_fixture: include_str!("fixtures/cert_manager_service.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: common::ProviderKind::K8s("v1.35.0"),
    dump_stem: "cert-manager.service",
};

#[test]
fn schema_from_tree_sitter() {
    common::assert_schema_fixture(&CASE);
}

#[test]
fn schema_validates_values_yaml() {
    common::assert_values_yaml_validates(&CASE);
}

#[test]
fn schema_keeps_default_rendered_service_metadata_typed() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAnnotations": {
                    "example.com/bad": 7
                }
            })
        ),
        "serviceAnnotations must stay a string map because prometheus.enabled defaults to true and podmonitor.enabled defaults to false: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "prometheus": {
                    "enabled": false
                },
                "serviceAnnotations": {
                    "example.com/bad": 7
                }
            })
        ),
        "serviceAnnotations should be unconstrained when the Service template is disabled: {schema}"
    );
}
