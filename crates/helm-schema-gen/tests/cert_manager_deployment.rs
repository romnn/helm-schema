#![recursion_limit = "4096"]

mod common;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/cert-manager/templates/deployment.yaml",
    values_path: "charts/cert-manager/values.yaml",
    expected_fixture: include_str!("fixtures/cert_manager_deployment.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: common::ProviderKind::K8s("v1.35.0"),
    dump_stem: "cert-manager.deployment",
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
fn schema_keeps_default_enabled_liveness_probe_fields_typed() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "livenessProbe": {
                    "failureThreshold": "eight"
                }
            })
        ),
        "livenessProbe.failureThreshold must stay integer-like because livenessProbe.enabled defaults to true: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "livenessProbe": {
                    "enabled": false,
                    "failureThreshold": "eight"
                }
            })
        ),
        "disabled livenessProbe fields should remain unconstrained because the template skips them: {schema}"
    );
}
