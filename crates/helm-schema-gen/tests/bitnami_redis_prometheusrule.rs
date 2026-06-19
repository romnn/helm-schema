mod common;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/bitnami-redis/templates/prometheusrule.yaml",
    values_path: "charts/bitnami-redis/values.yaml",
    expected_fixture: include_str!("fixtures/bitnami_redis_prometheusrule.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
        helper_template_dirs: &[("charts/common/templates", "tpl")],
        file_sources: &[],
    },
    provider: common::ProviderKind::CrdK8s("v1.35.0"),
    dump_stem: "bitnami-redis.prometheusrule",
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
fn schema_keeps_prometheus_rule_namespace_guarded() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "metrics": {
                    "enabled": true,
                    "prometheusRule": {
                        "enabled": true,
                        "namespace": 7
                    }
                }
            })
        ),
        "metrics.prometheusRule.namespace must stay namespace/string-like when the PrometheusRule renders: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "metrics": {
                    "enabled": false,
                    "prometheusRule": {
                        "enabled": true,
                        "namespace": 7
                    }
                }
            })
        ),
        "PrometheusRule-only namespace should remain unconstrained when metrics disables the resource: {schema}"
    );
}
