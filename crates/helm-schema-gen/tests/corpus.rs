#![recursion_limit = "4096"]

mod common;

use common::cases;
use helm_schema_k8s::{Chain, Diagnostic, DiagnosticSink, KubernetesJsonSchemaProvider};

#[test]
fn schema_fixtures_match() {
    for case in cases::STANDARD_SCHEMA_CASES {
        common::assert_schema_fixture(case);
    }
}

#[test]
fn values_yaml_validates_against_generated_schemas() {
    for case in cases::VALUES_VALIDATION_CASES {
        common::assert_values_yaml_validates(case);
    }
}

#[test]
fn helm_templates_render_successfully() {
    for case in cases::HELM_RENDER_CASES {
        common::assert_helm_render_case(case);
    }
}

macro_rules! schema_behavior_test {
    ($name:ident, $case:expr) => {
        #[test]
        fn $name() {
            common::assert_schema_behavior_case(&$case);
        }
    };
}

schema_behavior_test!(
    bitnami_redis_prometheusrule_behavior,
    cases::BITNAMI_REDIS_PROMETHEUSRULE_BEHAVIOR
);
schema_behavior_test!(
    cert_manager_deployment_behavior,
    cases::CERT_MANAGER_DEPLOYMENT_BEHAVIOR
);
schema_behavior_test!(
    cert_manager_service_behavior,
    cases::CERT_MANAGER_SERVICE_BEHAVIOR
);
schema_behavior_test!(
    nats_service_account_behavior,
    cases::NATS_SERVICE_ACCOUNT_BEHAVIOR
);
schema_behavior_test!(nats_service_behavior, cases::NATS_SERVICE_BEHAVIOR);
schema_behavior_test!(
    signoz_postgresql_secrets_behavior,
    cases::SIGNOZ_POSTGRESQL_SECRETS_BEHAVIOR
);
schema_behavior_test!(
    signoz_zookeeper_statefulset_behavior,
    cases::SIGNOZ_ZOOKEEPER_STATEFULSET_BEHAVIOR
);
schema_behavior_test!(
    signoz_zookeeper_service_behavior,
    cases::SIGNOZ_ZOOKEEPER_SVC_BEHAVIOR
);
schema_behavior_test!(
    zalando_clusterrolebinding_behavior,
    cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING_BEHAVIOR
);
schema_behavior_test!(
    zalando_clusterrole_behavior,
    cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE_BEHAVIOR
);
schema_behavior_test!(
    zalando_priority_class_behavior,
    cases::ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS_BEHAVIOR
);

macro_rules! rendered_manifest_validation_test {
    ($name:ident, $case:expr) => {
        #[test]
        fn $name() {
            common::assert_rendered_manifest_validation_case(&$case);
        }
    };
}

rendered_manifest_validation_test!(
    rendered_nats_operator_rbac_validates_default,
    cases::RENDERED_NATS_OPERATOR_RBAC_DEFAULT
);
rendered_manifest_validation_test!(
    rendered_nats_operator_rbac_validates_cluster_scoped,
    cases::RENDERED_NATS_OPERATOR_RBAC_CLUSTER_SCOPED
);
rendered_manifest_validation_test!(
    rendered_surveyor_configmap_validates,
    cases::RENDERED_SURVEYOR_CONFIGMAP
);
rendered_manifest_validation_test!(
    rendered_surveyor_hpa_validates,
    cases::RENDERED_SURVEYOR_HPA
);
rendered_manifest_validation_test!(
    rendered_surveyor_service_monitor_validates,
    cases::RENDERED_SURVEYOR_SERVICE_MONITOR
);
rendered_manifest_validation_test!(
    rendered_zalando_postgres_operator_ui_ingress_validates,
    cases::RENDERED_ZALANDO_POSTGRES_OPERATOR_UI_INGRESS
);

#[test]
fn warns_when_hpa_v2beta1_schema_missing_in_newer_k8s_bundle() {
    let case = cases::SURVEYOR_HPA;
    let src = test_util::read_testdata(case.template_path);
    let values_yaml = test_util::read_testdata(case.values_path);
    let idx = common::build_define_index(case.define_sources, case.helper_parse_mode);
    let ir = helm_schema_ir::SymbolicIrContext::new(&idx).generate_contract_ir(&src);

    let diagnostics = DiagnosticSink::new();
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_allow_download(true)
        .with_diagnostic_sink(diagnostics.clone());
    let chain = Chain::new(vec![Box::new(k8s_provider)]).with_diagnostic_sink(diagnostics.clone());

    let _schema = common::generate_schema_with_values_yaml(ir, &chain, Some(&values_yaml));

    let actual = diagnostics.snapshot();
    let hint = actual
        .iter()
        .find_map(|diagnostic| match diagnostic {
            Diagnostic::MissingSchema {
                kind,
                api_version,
                k8s_versions_tried,
                hint,
                ..
            } if kind == "HorizontalPodAutoscaler"
                && api_version == "autoscaling/v2beta1"
                && k8s_versions_tried.iter().any(|version| version == "v1.35.0") =>
            {
                Some(hint.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "expected a missing-upstream-schema warning for HPA autoscaling/v2beta1 in v1.35.0; got: {actual:?}"
            )
        });

    assert!(
        hint.as_deref()
            .is_some_and(|text| text.contains("removed in Kubernetes v1.25+")),
        "expected warning hint to mention removal in Kubernetes v1.25+; got: {hint:?}"
    );
}
