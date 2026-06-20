use super::{HelperParseMode, ProviderKind, SchemaCorpusCase};

const SURVEYOR_CONFIGMAP_FIXTURE_VALUES: &str = r#"
nameOverride: ""
fullnameOverride: ""
config:
  jetstream:
    enabled: false
    accounts:
      - name: test
        username: username
        password: password
        tls:
          ca: ca.crt
          cert: tls.crt
          key: tls.key
"#;

pub const BITNAMI_REDIS_NETWORKPOLICY: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/bitnami-redis/templates/networkpolicy.yaml",
    values_path: "charts/bitnami-redis/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/bitnami_redis_networkpolicy.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
        helper_template_dirs: &[("charts/common/templates", "tpl")],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "bitnami-redis.networkpolicy",
};

pub const BITNAMI_REDIS_PROMETHEUSRULE: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/bitnami-redis/templates/prometheusrule.yaml",
    values_path: "charts/bitnami-redis/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/bitnami_redis_prometheusrule.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
        helper_template_dirs: &[("charts/common/templates", "tpl")],
        file_sources: &[],
    },
    provider: ProviderKind::CrdK8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "bitnami-redis.prometheusrule",
};

pub const CERT_MANAGER_DEPLOYMENT: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/cert-manager/templates/deployment.yaml",
    values_path: "charts/cert-manager/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/cert_manager_deployment.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "cert-manager.deployment",
};

pub const CERT_MANAGER_SERVICE: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/cert-manager/templates/service.yaml",
    values_path: "charts/cert-manager/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/cert_manager_service.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "cert-manager.service",
};

pub const NATS_OPERATOR_RBAC: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/nats-operator/templates/rbac.yaml",
    values_path: "charts/nats-operator/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/nats_operator_rbac.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/nats-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "nats-operator.rbac",
};

pub const NATS_SERVICE_ACCOUNT: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/nats/templates/service-account.yaml",
    values_path: "charts/nats/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/nats_service_account.schema.json"),
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
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "nats-service-account",
};

pub const NATS_SERVICE: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/nats/templates/service.yaml",
    values_path: "charts/nats/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/nats_service.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &[
            "charts/nats/templates/_helpers.tpl",
            "charts/nats/templates/_jsonpatch.tpl",
            "charts/nats/templates/_tplYaml.tpl",
            "charts/nats/templates/_toPrettyRawJson.tpl",
        ],
        helper_template_dirs: &[],
        file_sources: &[("files/service.yaml", "charts/nats/files/service.yaml")],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "nats-service",
};

pub const SIGNOZ_POSTGRESQL_SECRETS: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml",
    values_path: "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/signoz_postgresql_secrets.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &[
            "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/_helpers.tpl",
        ],
        helper_template_dirs: &[(
            "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/charts/common/templates",
            "tpl",
        )],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "signoz-postgresql-secrets",
};

pub const SIGNOZ_ZOOKEEPER_STATEFULSET: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml",
    values_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/signoz_zookeeper_statefulset.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &[
            "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/_helpers.tpl",
        ],
        helper_template_dirs: &[(
            "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates",
            "tpl",
        )],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "signoz-zookeeper-statefulset",
};

pub const SIGNOZ_ZOOKEEPER_SVC: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml",
    values_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/signoz_zookeeper_svc.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &[
            "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/_helpers.tpl",
        ],
        helper_template_dirs: &[(
            "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates",
            "tpl",
        )],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "signoz-zookeeper-svc",
};

pub const SURVEYOR_CONFIGMAP: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/surveyor/templates/configmap.yaml",
    values_path: "charts/surveyor/values.yaml",
    fixture_values_yaml: Some(SURVEYOR_CONFIGMAP_FIXTURE_VALUES),
    expected_fixture: include_str!("../fixtures/surveyor_configmap.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Strict,
    dump_stem: "surveyor.configmap",
};

pub const SURVEYOR_HPA: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/surveyor/templates/hpa.yaml",
    values_path: "charts/surveyor/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/surveyor_hpa.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.24.0"),
    helper_parse_mode: HelperParseMode::Strict,
    dump_stem: "surveyor.hpa",
};

pub const SURVEYOR_SERVICE_MONITOR: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/surveyor/templates/serviceMonitor.yaml",
    values_path: "charts/surveyor/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/surveyor_service_monitor.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::CrdK8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Strict,
    dump_stem: "surveyor.service-monitor",
};

pub const ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING: SchemaCorpusCase<'static> =
    SchemaCorpusCase {
        template_path: "charts/zalando-postgres-operator/templates/clusterrolebinding.yaml",
        values_path: "charts/zalando-postgres-operator/values.yaml",
        fixture_values_yaml: None,
        expected_fixture: include_str!(
            "../fixtures/zalando_postgres_operator_clusterrolebinding.schema.json"
        ),
        define_sources: test_util::DefineSourceSpec {
            helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
            helper_template_dirs: &[],
            file_sources: &[],
        },
        provider: ProviderKind::K8s("v1.35.0"),
        helper_parse_mode: HelperParseMode::Lenient,
        dump_stem: "zalando-postgres-operator.clusterrolebinding",
    };

pub const ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/clusterrole.yaml",
    values_path: "charts/zalando-postgres-operator/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/zalando_postgres_operator_clusterrole.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "zalando-postgres-operator.clusterrole",
};

pub const ZALANDO_POSTGRES_OPERATOR_DEPLOYMENT: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/deployment.yaml",
    values_path: "charts/zalando-postgres-operator/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/zalando_postgres_operator_deployment.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "zalando-postgres-operator.deployment",
};

pub const ZALANDO_POSTGRES_OPERATOR_UI_INGRESS: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/zalando-postgres-operator-ui/templates/ingress.yaml",
    values_path: "charts/zalando-postgres-operator-ui/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/zalando_postgres_operator_ui_ingress.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator-ui/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "zalando-postgres-operator-ui.ingress",
};

pub const ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS: SchemaCorpusCase<'static> =
    SchemaCorpusCase {
        template_path: "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
        values_path: "charts/zalando-postgres-operator/values.yaml",
        fixture_values_yaml: None,
        expected_fixture: include_str!(
            "../fixtures/zalando_postgres_operator_postgres_pod_priority_class.schema.json"
        ),
        define_sources: test_util::DefineSourceSpec {
            helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
            helper_template_dirs: &[],
            file_sources: &[],
        },
        provider: ProviderKind::K8s("v1.35.0"),
        helper_parse_mode: HelperParseMode::Lenient,
        dump_stem: "zalando-postgres-operator.postgres-pod-priority-class",
    };

pub const STANDARD_SCHEMA_CASES: &[SchemaCorpusCase<'static>] = &[
    BITNAMI_REDIS_NETWORKPOLICY,
    BITNAMI_REDIS_PROMETHEUSRULE,
    CERT_MANAGER_DEPLOYMENT,
    CERT_MANAGER_SERVICE,
    NATS_OPERATOR_RBAC,
    NATS_SERVICE_ACCOUNT,
    NATS_SERVICE,
    SIGNOZ_POSTGRESQL_SECRETS,
    SIGNOZ_ZOOKEEPER_STATEFULSET,
    SIGNOZ_ZOOKEEPER_SVC,
    SURVEYOR_CONFIGMAP,
    SURVEYOR_HPA,
    SURVEYOR_SERVICE_MONITOR,
    ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING,
    ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE,
    ZALANDO_POSTGRES_OPERATOR_DEPLOYMENT,
    ZALANDO_POSTGRES_OPERATOR_UI_INGRESS,
    ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS,
];

pub const VALUES_VALIDATION_CASES: &[SchemaCorpusCase<'static>] = &[
    BITNAMI_REDIS_NETWORKPOLICY,
    BITNAMI_REDIS_PROMETHEUSRULE,
    CERT_MANAGER_DEPLOYMENT,
    CERT_MANAGER_SERVICE,
    NATS_OPERATOR_RBAC,
    NATS_SERVICE_ACCOUNT,
    NATS_SERVICE,
    SIGNOZ_POSTGRESQL_SECRETS,
    SIGNOZ_ZOOKEEPER_STATEFULSET,
    SIGNOZ_ZOOKEEPER_SVC,
    SURVEYOR_CONFIGMAP,
    SURVEYOR_HPA,
    SURVEYOR_SERVICE_MONITOR,
    ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING,
    ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE,
    ZALANDO_POSTGRES_OPERATOR_DEPLOYMENT,
    ZALANDO_POSTGRES_OPERATOR_UI_INGRESS,
    ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS,
];
