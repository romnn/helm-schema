use super::{
    HelmRenderCase, HelperParseMode, ProviderKind, RenderedManifestValidationCase,
    RenderedSchemaProviderKind, SchemaBehaviorCase, SchemaCorpusCase, SchemaExpectation,
};

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

pub const DICT_CONFIG_PDB: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/dict-config/templates/pdb.yaml",
    values_path: "charts/dict-config/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/dict_config_pdb.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/dict-config/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "dict-config.pdb",
};

pub const DICT_CONFIG_INGRESS: SchemaCorpusCase<'static> = SchemaCorpusCase {
    template_path: "charts/dict-config/templates/ingress.yaml",
    values_path: "charts/dict-config/values.yaml",
    fixture_values_yaml: None,
    expected_fixture: include_str!("../fixtures/dict_config_ingress.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/dict-config/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: ProviderKind::K8s("v1.35.0"),
    helper_parse_mode: HelperParseMode::Lenient,
    dump_stem: "dict-config.ingress",
};

pub const STANDARD_SCHEMA_CASES: &[SchemaCorpusCase<'static>] = &[
    BITNAMI_REDIS_NETWORKPOLICY,
    BITNAMI_REDIS_PROMETHEUSRULE,
    DICT_CONFIG_PDB,
    DICT_CONFIG_INGRESS,
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
    DICT_CONFIG_PDB,
    DICT_CONFIG_INGRESS,
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

pub const HELM_RENDER_CASES: &[HelmRenderCase<'static>] = &[
    HelmRenderCase {
        name: "nats-operator rbac default",
        chart_path: "charts/nats-operator",
        show_only: Some("templates/rbac.yaml"),
        extra_args: &[],
    },
    HelmRenderCase {
        name: "nats-operator rbac cluster scoped",
        chart_path: "charts/nats-operator",
        show_only: Some("templates/rbac.yaml"),
        extra_args: &["--set", "clusterScoped=true"],
    },
    HelmRenderCase {
        name: "nats service account",
        chart_path: "charts/nats",
        show_only: Some("templates/service-account.yaml"),
        extra_args: &["--set", "serviceAccount.enabled=true"],
    },
    HelmRenderCase {
        name: "nats service",
        chart_path: "charts/nats",
        show_only: Some("templates/service.yaml"),
        extra_args: &[],
    },
    HelmRenderCase {
        name: "signoz postgresql secrets",
        chart_path: "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql",
        show_only: Some("templates/secrets.yaml"),
        extra_args: &[],
    },
    HelmRenderCase {
        name: "signoz zookeeper statefulset",
        chart_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper",
        show_only: Some("templates/statefulset.yaml"),
        extra_args: &[],
    },
    HelmRenderCase {
        name: "signoz zookeeper service",
        chart_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper",
        show_only: Some("templates/svc.yaml"),
        extra_args: &[],
    },
    HelmRenderCase {
        name: "surveyor configmap",
        chart_path: "charts/surveyor",
        show_only: Some("templates/configmap.yaml"),
        extra_args: &[
            "--set",
            "config.jetstream.enabled=true",
            "--set",
            "config.jetstream.accounts[0].name=test",
            "--set",
            "config.jetstream.accounts[0].username=username",
            "--set",
            "config.jetstream.accounts[0].password=password",
            "--set",
            "config.jetstream.accounts[0].tls.secret.name=test-user-tls",
            "--set",
            "config.jetstream.accounts[0].tls.ca=ca.crt",
            "--set",
            "config.jetstream.accounts[0].tls.cert=tls.crt",
            "--set",
            "config.jetstream.accounts[0].tls.key=tls.key",
        ],
    },
    HelmRenderCase {
        name: "surveyor hpa",
        chart_path: "charts/surveyor",
        show_only: Some("templates/hpa.yaml"),
        extra_args: &[
            "--set",
            "autoscaling.enabled=true",
            "--kube-version",
            "1.24.0",
        ],
    },
    HelmRenderCase {
        name: "surveyor service monitor",
        chart_path: "charts/surveyor",
        show_only: Some("templates/serviceMonitor.yaml"),
        extra_args: &[
            "--set",
            "serviceMonitor.enabled=true",
            "--kube-version",
            "1.29.0",
        ],
    },
    HelmRenderCase {
        name: "zalando postgres operator clusterrolebinding",
        chart_path: "charts/zalando-postgres-operator",
        show_only: Some("templates/clusterrolebinding.yaml"),
        extra_args: &[],
    },
    HelmRenderCase {
        name: "zalando postgres operator clusterrole",
        chart_path: "charts/zalando-postgres-operator",
        show_only: Some("templates/clusterrole.yaml"),
        extra_args: &[],
    },
    HelmRenderCase {
        name: "zalando postgres operator deployment",
        chart_path: "charts/zalando-postgres-operator",
        show_only: Some("templates/deployment.yaml"),
        extra_args: &[],
    },
    HelmRenderCase {
        name: "zalando postgres operator ui ingress",
        chart_path: "charts/zalando-postgres-operator-ui",
        show_only: Some("templates/ingress.yaml"),
        extra_args: &["--set", "ingress.enabled=true", "--kube-version", "1.29.0"],
    },
    HelmRenderCase {
        name: "zalando postgres operator priority class",
        chart_path: "charts/zalando-postgres-operator",
        show_only: Some("templates/postgres-pod-priority-class.yaml"),
        extra_args: &[],
    },
];

pub const BITNAMI_REDIS_PROMETHEUSRULE_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: BITNAMI_REDIS_PROMETHEUSRULE,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"metrics":{"enabled":true,"prometheusRule":{"enabled":true,"namespace":7}}}"#,
            accepted: false,
            message: "metrics.prometheusRule.namespace must stay namespace/string-like when the PrometheusRule renders",
        },
        SchemaExpectation {
            instance: r#"{"metrics":{"enabled":false,"prometheusRule":{"enabled":true,"namespace":7}}}"#,
            accepted: true,
            message: "PrometheusRule-only namespace should remain unconstrained when metrics disables the resource",
        },
    ],
};

pub const CERT_MANAGER_DEPLOYMENT_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: CERT_MANAGER_DEPLOYMENT,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"livenessProbe":{"failureThreshold":"eight"}}"#,
            accepted: false,
            message: "livenessProbe.failureThreshold must stay integer-like because livenessProbe.enabled defaults to true",
        },
        SchemaExpectation {
            instance: r#"{"livenessProbe":{"enabled":false,"failureThreshold":"eight"}}"#,
            accepted: true,
            message: "disabled livenessProbe fields should remain unconstrained because the template skips them",
        },
    ],
};

pub const CERT_MANAGER_SERVICE_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: CERT_MANAGER_SERVICE,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"serviceAnnotations":{"example.com/bad":7}}"#,
            accepted: false,
            message: "serviceAnnotations must stay a string map when the Service renders by default",
        },
        SchemaExpectation {
            instance: r#"{"prometheus":{"enabled":false},"serviceAnnotations":{"example.com/bad":7}}"#,
            accepted: true,
            message: "serviceAnnotations should be unconstrained when the Service template is disabled",
        },
    ],
};

pub const NATS_SERVICE_ACCOUNT_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: NATS_SERVICE_ACCOUNT,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"serviceAccount":{"enabled":true,"name":7}}"#,
            accepted: false,
            message: "serviceAccount.name must stay string-like when ServiceAccount rendering is enabled",
        },
        SchemaExpectation {
            instance: r#"{"serviceAccount":{"enabled":false,"name":7}}"#,
            accepted: true,
            message: "serviceAccount.name should remain unconstrained when the ServiceAccount is disabled",
        },
    ],
};

pub const NATS_SERVICE_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: NATS_SERVICE,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"service":{"name":7}}"#,
            accepted: false,
            message: "service.name must stay string-like when service.enabled defaults to true",
        },
        SchemaExpectation {
            instance: r#"{"nameOverride":7}"#,
            accepted: false,
            message: "nameOverride must stay string-like when the Service is rendered by default",
        },
        SchemaExpectation {
            instance: r#"{"service":{"enabled":false,"name":7},"nameOverride":7}"#,
            accepted: true,
            message: "service-only name inputs should remain unconstrained when the Service is disabled",
        },
    ],
};

pub const SIGNOZ_POSTGRESQL_SECRETS_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: SIGNOZ_POSTGRESQL_SECRETS,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"serviceBindings":{"enabled":true},"architecture":"replication","primary":{"name":7}}"#,
            accepted: false,
            message: "primary.name must stay string-like when service-binding host rendering uses it",
        },
        SchemaExpectation {
            instance: r#"{"serviceBindings":{"enabled":false},"architecture":"replication","primary":{"name":7}}"#,
            accepted: true,
            message: "primary.name should remain unconstrained when service bindings are disabled",
        },
    ],
};

pub const SIGNOZ_ZOOKEEPER_STATEFULSET_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: SIGNOZ_ZOOKEEPER_STATEFULSET,
    expectations: &[SchemaExpectation {
        instance: r#"{"containerSecurityContext":{"runAsUser":"root"}}"#,
        accepted: false,
        message: "containerSecurityContext.runAsUser must stay integer-like because containerSecurityContext.enabled defaults to true",
    }],
};

pub const SIGNOZ_ZOOKEEPER_SVC_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: SIGNOZ_ZOOKEEPER_SVC,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"service":{"ports":{"client":"client-port"}}}"#,
            accepted: false,
            message: "service.ports.client must stay integer-like because disableBaseClientPort defaults to false",
        },
        SchemaExpectation {
            instance: r#"{"service":{"disableBaseClientPort":true,"ports":{"client":"client-port"}}}"#,
            accepted: true,
            message: "service.ports.client should be unconstrained when disableBaseClientPort removes that Service port",
        },
        SchemaExpectation {
            instance: r#"{"tls":{"client":{"enabled":true}},"service":{"ports":{"tls":"tls-port"}}}"#,
            accepted: false,
            message: "service.ports.tls must stay integer-like when tls.client.enabled renders the TLS port",
        },
    ],
};

pub const ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING_BEHAVIOR: SchemaBehaviorCase<'static> =
    SchemaBehaviorCase {
        schema_case: ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING,
        expectations: &[
            SchemaExpectation {
                instance: r#"{"serviceAccount":{"name":7}}"#,
                accepted: false,
                message: "serviceAccount.name must stay string-like when rbac.create defaults to true",
            },
            SchemaExpectation {
                instance: r#"{"rbac":{"create":false},"serviceAccount":{"name":7}}"#,
                accepted: true,
                message: "serviceAccount.name should remain unconstrained when ClusterRoleBinding rendering is disabled",
            },
        ],
    };

pub const ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE_BEHAVIOR: SchemaBehaviorCase<'static> =
    SchemaBehaviorCase {
        schema_case: ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE,
        expectations: &[
            SchemaExpectation {
                instance: r#"{"serviceAccount":{"name":7}}"#,
                accepted: false,
                message: "serviceAccount.name must stay string-like when rbac.create defaults to true",
            },
            SchemaExpectation {
                instance: r#"{"rbac":{"create":false},"serviceAccount":{"name":7}}"#,
                accepted: true,
                message: "serviceAccount.name should remain unconstrained when ClusterRole rendering is disabled",
            },
        ],
    };

pub const ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS_BEHAVIOR: SchemaBehaviorCase<
    'static,
> = SchemaBehaviorCase {
    schema_case: ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"podPriorityClassName":{"name":7}}"#,
            accepted: false,
            message: "podPriorityClassName.name must stay string-like when create defaults to true",
        },
        SchemaExpectation {
            instance: r#"{"podPriorityClassName":{"priority":"high"}}"#,
            accepted: false,
            message: "podPriorityClassName.priority must stay integer-like when create defaults to true",
        },
        SchemaExpectation {
            instance: r#"{"podPriorityClassName":{"create":false,"name":7,"priority":"high"}}"#,
            accepted: true,
            message: "PriorityClass fields should remain unconstrained when PriorityClass rendering is disabled",
        },
    ],
};

pub const RENDERED_NATS_OPERATOR_RBAC_DEFAULT: RenderedManifestValidationCase<'static> =
    RenderedManifestValidationCase {
        render: HelmRenderCase {
            name: "nats-operator rbac default validation",
            chart_path: "charts/nats-operator",
            show_only: Some("templates/rbac.yaml"),
            extra_args: &[],
        },
        provider: RenderedSchemaProviderKind::K8s("v1.35.0"),
    };

pub const RENDERED_NATS_OPERATOR_RBAC_CLUSTER_SCOPED: RenderedManifestValidationCase<'static> =
    RenderedManifestValidationCase {
        render: HelmRenderCase {
            name: "nats-operator rbac cluster scoped validation",
            chart_path: "charts/nats-operator",
            show_only: Some("templates/rbac.yaml"),
            extra_args: &["--set", "clusterScoped=true"],
        },
        provider: RenderedSchemaProviderKind::K8s("v1.35.0"),
    };

pub const RENDERED_SURVEYOR_CONFIGMAP: RenderedManifestValidationCase<'static> =
    RenderedManifestValidationCase {
        render: HelmRenderCase {
            name: "surveyor configmap validation",
            chart_path: "charts/surveyor",
            show_only: Some("templates/configmap.yaml"),
            extra_args: &[
                "--set",
                "config.jetstream.enabled=true",
                "--set",
                "config.jetstream.accounts[0].name=test",
                "--set",
                "config.jetstream.accounts[0].username=username",
                "--set",
                "config.jetstream.accounts[0].password=password",
                "--set",
                "config.jetstream.accounts[0].tls.secret.name=test-user-tls",
                "--set",
                "config.jetstream.accounts[0].tls.ca=ca.crt",
                "--set",
                "config.jetstream.accounts[0].tls.cert=tls.crt",
                "--set",
                "config.jetstream.accounts[0].tls.key=tls.key",
            ],
        },
        provider: RenderedSchemaProviderKind::K8s("v1.35.0"),
    };

pub const RENDERED_SURVEYOR_HPA: RenderedManifestValidationCase<'static> =
    RenderedManifestValidationCase {
        render: HelmRenderCase {
            name: "surveyor hpa validation",
            chart_path: "charts/surveyor",
            show_only: Some("templates/hpa.yaml"),
            extra_args: &[
                "--set",
                "autoscaling.enabled=true",
                "--kube-version",
                "1.24.0",
            ],
        },
        provider: RenderedSchemaProviderKind::K8s("v1.24.0"),
    };

pub const RENDERED_SURVEYOR_SERVICE_MONITOR: RenderedManifestValidationCase<'static> =
    RenderedManifestValidationCase {
        render: HelmRenderCase {
            name: "surveyor service monitor validation",
            chart_path: "charts/surveyor",
            show_only: Some("templates/serviceMonitor.yaml"),
            extra_args: &[
                "--set",
                "serviceMonitor.enabled=true",
                "--kube-version",
                "1.29.0",
            ],
        },
        provider: RenderedSchemaProviderKind::CrdCatalog,
    };

pub const RENDERED_ZALANDO_POSTGRES_OPERATOR_UI_INGRESS: RenderedManifestValidationCase<'static> =
    RenderedManifestValidationCase {
        render: HelmRenderCase {
            name: "zalando postgres operator ui ingress validation",
            chart_path: "charts/zalando-postgres-operator-ui",
            show_only: Some("templates/ingress.yaml"),
            extra_args: &["--set", "ingress.enabled=true", "--kube-version", "1.29.0"],
        },
        provider: RenderedSchemaProviderKind::K8s("v1.35.0"),
    };
