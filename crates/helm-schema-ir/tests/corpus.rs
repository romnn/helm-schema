#![recursion_limit = "1024"]

mod common;

macro_rules! ir_corpus_case {
    ($name:ident, $case:expr) => {
        #[test]
        fn $name() {
            common::assert_ir_fixture($case);
        }
    };
}

ir_corpus_case!(
    bitnami_redis_networkpolicy,
    common::cases::BITNAMI_REDIS_NETWORKPOLICY
);
ir_corpus_case!(
    bitnami_redis_prometheusrule,
    common::cases::BITNAMI_REDIS_PROMETHEUSRULE
);
ir_corpus_case!(
    cert_manager_deployment,
    common::cases::CERT_MANAGER_DEPLOYMENT
);
ir_corpus_case!(cert_manager_service, common::cases::CERT_MANAGER_SERVICE);
ir_corpus_case!(nats_operator_rbac, common::cases::NATS_OPERATOR_RBAC);
ir_corpus_case!(nats_service, common::cases::NATS_SERVICE);
ir_corpus_case!(nats_service_account, common::cases::NATS_SERVICE_ACCOUNT);
ir_corpus_case!(
    signoz_postgresql_secrets,
    common::cases::SIGNOZ_POSTGRESQL_SECRETS
);
ir_corpus_case!(
    signoz_zookeeper_statefulset,
    common::cases::SIGNOZ_ZOOKEEPER_STATEFULSET
);
ir_corpus_case!(signoz_zookeeper_svc, common::cases::SIGNOZ_ZOOKEEPER_SVC);
ir_corpus_case!(surveyor_configmap, common::cases::SURVEYOR_CONFIGMAP);
ir_corpus_case!(surveyor_hpa, common::cases::SURVEYOR_HPA);
ir_corpus_case!(
    surveyor_service_monitor,
    common::cases::SURVEYOR_SERVICE_MONITOR
);
ir_corpus_case!(
    zalando_postgres_operator_clusterrole,
    common::cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE
);
ir_corpus_case!(
    zalando_postgres_operator_clusterrolebinding,
    common::cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING
);
ir_corpus_case!(
    zalando_postgres_operator_deployment,
    common::cases::ZALANDO_POSTGRES_OPERATOR_DEPLOYMENT
);
ir_corpus_case!(
    zalando_postgres_operator_postgres_pod_priority_class,
    common::cases::ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS
);
ir_corpus_case!(
    zalando_postgres_operator_ui_ingress,
    common::cases::ZALANDO_POSTGRES_OPERATOR_UI_INGRESS
);
