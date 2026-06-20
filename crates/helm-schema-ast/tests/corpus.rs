mod common;

macro_rules! ast_corpus_case {
    ($name:ident, $case:expr) => {
        #[test]
        fn $name() {
            common::assert_ast_fixture($case);
        }
    };
}

ast_corpus_case!(
    bitnami_redis_networkpolicy,
    common::cases::BITNAMI_REDIS_NETWORKPOLICY
);
ast_corpus_case!(
    bitnami_redis_prometheusrule,
    common::cases::BITNAMI_REDIS_PROMETHEUSRULE
);
ast_corpus_case!(
    cert_manager_deployment,
    common::cases::CERT_MANAGER_DEPLOYMENT
);
ast_corpus_case!(cert_manager_service, common::cases::CERT_MANAGER_SERVICE);
ast_corpus_case!(nats_operator_rbac, common::cases::NATS_OPERATOR_RBAC);
ast_corpus_case!(nats_service, common::cases::NATS_SERVICE);
ast_corpus_case!(nats_service_account, common::cases::NATS_SERVICE_ACCOUNT);
ast_corpus_case!(
    signoz_postgresql_secrets,
    common::cases::SIGNOZ_POSTGRESQL_SECRETS
);
ast_corpus_case!(
    signoz_zookeeper_statefulset,
    common::cases::SIGNOZ_ZOOKEEPER_STATEFULSET
);
ast_corpus_case!(signoz_zookeeper_svc, common::cases::SIGNOZ_ZOOKEEPER_SVC);
ast_corpus_case!(surveyor_configmap, common::cases::SURVEYOR_CONFIGMAP);
ast_corpus_case!(surveyor_hpa, common::cases::SURVEYOR_HPA);
ast_corpus_case!(
    surveyor_service_monitor,
    common::cases::SURVEYOR_SERVICE_MONITOR
);
ast_corpus_case!(
    zalando_postgres_operator_clusterrole,
    common::cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE
);
ast_corpus_case!(
    zalando_postgres_operator_clusterrolebinding,
    common::cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING
);
ast_corpus_case!(
    zalando_postgres_operator_deployment,
    common::cases::ZALANDO_POSTGRES_OPERATOR_DEPLOYMENT
);
ast_corpus_case!(
    zalando_postgres_operator_postgres_pod_priority_class,
    common::cases::ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS
);
ast_corpus_case!(
    zalando_postgres_operator_ui_ingress,
    common::cases::ZALANDO_POSTGRES_OPERATOR_UI_INGRESS
);
