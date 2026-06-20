use super::AstCorpusCase;

pub const BITNAMI_REDIS_NETWORKPOLICY: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/bitnami-redis/templates/networkpolicy.yaml",
    expected_fixture: include_str!("../fixtures/bitnami_redis_networkpolicy.sexpr"),
};

pub const BITNAMI_REDIS_PROMETHEUSRULE: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/bitnami-redis/templates/prometheusrule.yaml",
    expected_fixture: include_str!("../fixtures/bitnami_redis_prometheusrule.sexpr"),
};

pub const CERT_MANAGER_DEPLOYMENT: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/cert-manager/templates/deployment.yaml",
    expected_fixture: include_str!("../fixtures/cert_manager_deployment.sexpr"),
};

pub const CERT_MANAGER_SERVICE: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/cert-manager/templates/service.yaml",
    expected_fixture: include_str!("../fixtures/cert_manager_service.sexpr"),
};

pub const NATS_OPERATOR_RBAC: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/nats-operator/templates/rbac.yaml",
    expected_fixture: include_str!("../fixtures/nats_operator_rbac.sexpr"),
};

pub const NATS_SERVICE: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/nats/templates/service.yaml",
    expected_fixture: include_str!("../fixtures/nats_service.sexpr"),
};

pub const NATS_SERVICE_ACCOUNT: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/nats/templates/service-account.yaml",
    expected_fixture: include_str!("../fixtures/nats_service_account.sexpr"),
};

pub const SIGNOZ_POSTGRESQL_SECRETS: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml",
    expected_fixture: include_str!("../fixtures/signoz_postgresql_secrets.sexpr"),
};

pub const SIGNOZ_ZOOKEEPER_STATEFULSET: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml",
    expected_fixture: include_str!("../fixtures/signoz_zookeeper_statefulset.sexpr"),
};

pub const SIGNOZ_ZOOKEEPER_SVC: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml",
    expected_fixture: include_str!("../fixtures/signoz_zookeeper_svc.sexpr"),
};

pub const SURVEYOR_CONFIGMAP: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/surveyor/templates/configmap.yaml",
    expected_fixture: include_str!("../fixtures/surveyor_configmap.sexpr"),
};

pub const SURVEYOR_HPA: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/surveyor/templates/hpa.yaml",
    expected_fixture: include_str!("../fixtures/surveyor_hpa.sexpr"),
};

pub const SURVEYOR_SERVICE_MONITOR: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/surveyor/templates/serviceMonitor.yaml",
    expected_fixture: include_str!("../fixtures/surveyor_service_monitor.sexpr"),
};

pub const ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/clusterrole.yaml",
    expected_fixture: include_str!("../fixtures/zalando_postgres_operator_clusterrole.sexpr"),
};

pub const ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/clusterrolebinding.yaml",
    expected_fixture: include_str!(
        "../fixtures/zalando_postgres_operator_clusterrolebinding.sexpr"
    ),
};

pub const ZALANDO_POSTGRES_OPERATOR_DEPLOYMENT: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/deployment.yaml",
    expected_fixture: include_str!("../fixtures/zalando_postgres_operator_deployment.sexpr"),
};

pub const ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS: AstCorpusCase<'static> =
    AstCorpusCase {
        template_path: "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
        expected_fixture: include_str!(
            "../fixtures/zalando_postgres_operator_postgres_pod_priority_class.sexpr"
        ),
    };

pub const ZALANDO_POSTGRES_OPERATOR_UI_INGRESS: AstCorpusCase<'static> = AstCorpusCase {
    template_path: "charts/zalando-postgres-operator-ui/templates/ingress.yaml",
    expected_fixture: include_str!("../fixtures/zalando_postgres_operator_ui_ingress.sexpr"),
};

pub const STANDARD_AST_CASES: &[AstCorpusCase<'static>] = &[
    BITNAMI_REDIS_NETWORKPOLICY,
    BITNAMI_REDIS_PROMETHEUSRULE,
    CERT_MANAGER_DEPLOYMENT,
    CERT_MANAGER_SERVICE,
    NATS_OPERATOR_RBAC,
    NATS_SERVICE,
    NATS_SERVICE_ACCOUNT,
    SIGNOZ_POSTGRESQL_SECRETS,
    SIGNOZ_ZOOKEEPER_STATEFULSET,
    SIGNOZ_ZOOKEEPER_SVC,
    SURVEYOR_CONFIGMAP,
    SURVEYOR_HPA,
    SURVEYOR_SERVICE_MONITOR,
    ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE,
    ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING,
    ZALANDO_POSTGRES_OPERATOR_DEPLOYMENT,
    ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS,
    ZALANDO_POSTGRES_OPERATOR_UI_INGRESS,
];
