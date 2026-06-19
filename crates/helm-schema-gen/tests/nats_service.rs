#![recursion_limit = "512"]

mod common;

use helm_schema_ast::TreeSitterParser;
use helm_schema_ir::SymbolicIrContext;

const TEMPLATE_PATH: &str = "charts/nats/templates/service.yaml";
const VALUES_PATH: &str = "charts/nats/values.yaml";
const NATS_DEFINE_SOURCES: test_util::DefineSourceSpec<'static> = test_util::DefineSourceSpec {
    helper_templates: &[
        "charts/nats/templates/_helpers.tpl",
        "charts/nats/templates/_jsonpatch.tpl",
        "charts/nats/templates/_tplYaml.tpl",
        "charts/nats/templates/_toPrettyRawJson.tpl",
    ],
    file_sources: &[("files/service.yaml", "charts/nats/files/service.yaml")],
};

#[test]
#[allow(clippy::too_many_lines)]
fn schema_from_tree_sitter() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let idx = common::build_define_index(&TreeSitterParser, NATS_DEFINE_SOURCES);
    let ir = SymbolicIrContext::new(&idx).generate_contract_ir(&src, &idx);
    let provider = common::production_k8s_chain("v1.35.0");
    let schema = common::generate_schema_with_values_yaml(ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
        let path = std::env::temp_dir().join("helm-schema.nats-service.schema.json");
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&actual).expect("json bytes"),
        )
        .expect("write schema dump");
    }

    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/nats_service.schema.json"))
            .expect("expected schema json");

    similar_asserts::assert_eq!(actual, expected);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/nats");
    let rendered = common::helm_template_render(&chart_dir, Some("templates/service.yaml"));
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn schema_validates_values_yaml() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let idx = common::build_define_index(&TreeSitterParser, NATS_DEFINE_SOURCES);
    let ir = SymbolicIrContext::new(&idx).generate_contract_ir(&src, &idx);
    let provider = common::production_k8s_chain("v1.35.0");
    let schema = common::generate_schema_with_values_yaml(ir, &provider, Some(&values_yaml));

    let errors = common::validate_values_yaml(&values_yaml, &schema);
    assert!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}

#[test]
fn schema_keeps_live_service_name_paths_typed() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let idx = common::build_define_index(&TreeSitterParser, NATS_DEFINE_SOURCES);
    let ir = SymbolicIrContext::new(&idx).generate_contract_ir(&src, &idx);
    let provider = common::production_k8s_chain("v1.35.0");
    let schema = common::generate_schema_with_values_yaml(ir, &provider, Some(&values_yaml));

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "service": {
                    "name": 7
                }
            })
        ),
        "service.name must stay string-like when service.enabled defaults to true: {schema}"
    );
    assert!(
        !common::schema_accepts_instance(&schema, &serde_json::json!({ "nameOverride": 7 })),
        "nameOverride must stay string-like when the Service is rendered by default: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "service": {
                    "enabled": false,
                    "name": 7
                },
                "nameOverride": 7
            })
        ),
        "Service-only name inputs should remain unconstrained when the Service is disabled: {schema}"
    );
}
