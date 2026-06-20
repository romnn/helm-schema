use super::IrCorpusCase;

pub const BITNAMI_REDIS_NETWORKPOLICY: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/bitnami-redis/templates/networkpolicy.yaml",
    expected_fixture: include_str!("../fixtures/bitnami_redis_networkpolicy.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
        helper_template_dirs: &[("charts/common/templates", "tpl")],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const BITNAMI_REDIS_PROMETHEUSRULE: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/bitnami-redis/templates/prometheusrule.yaml",
    expected_fixture: include_str!("../fixtures/bitnami_redis_prometheusrule.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
        helper_template_dirs: &[("charts/common/templates", "tpl")],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const CERT_MANAGER_DEPLOYMENT: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/cert-manager/templates/deployment.yaml",
    expected_fixture: include_str!("../fixtures/cert_manager_deployment.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const CERT_MANAGER_SERVICE: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/cert-manager/templates/service.yaml",
    expected_fixture: include_str!("../fixtures/cert_manager_service.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const NATS_OPERATOR_RBAC: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/nats-operator/templates/rbac.yaml",
    expected_fixture: include_str!("../fixtures/nats_operator_rbac.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/nats-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const NATS_SERVICE: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/nats/templates/service.yaml",
    expected_fixture: include_str!("../fixtures/nats_service.ir.json"),
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
    dump_env: "SYMBOLIC_DUMP",
};

pub const NATS_SERVICE_ACCOUNT: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/nats/templates/service-account.yaml",
    expected_fixture: include_str!("../fixtures/nats_service_account.ir.json"),
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
    dump_env: "SYMBOLIC_DUMP",
};

pub const SIGNOZ_POSTGRESQL_SECRETS: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml",
    expected_fixture: include_str!("../fixtures/signoz_postgresql_secrets.ir.json"),
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
    dump_env: "SYMBOLIC_DUMP",
};

pub const SIGNOZ_ZOOKEEPER_STATEFULSET: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml",
    expected_fixture: include_str!("../fixtures/signoz_zookeeper_statefulset.ir.json"),
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
    dump_env: "IR_DUMP",
};

pub const SIGNOZ_ZOOKEEPER_SVC: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml",
    expected_fixture: include_str!("../fixtures/signoz_zookeeper_svc.ir.json"),
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
    dump_env: "SYMBOLIC_DUMP",
};

pub const SURVEYOR_CONFIGMAP: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/surveyor/templates/configmap.yaml",
    expected_fixture: include_str!("../fixtures/surveyor_configmap.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const SURVEYOR_HPA: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/surveyor/templates/hpa.yaml",
    expected_fixture: include_str!("../fixtures/surveyor_hpa.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const SURVEYOR_SERVICE_MONITOR: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/surveyor/templates/serviceMonitor.yaml",
    expected_fixture: include_str!("../fixtures/surveyor_service_monitor.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/clusterrole.yaml",
    expected_fixture: include_str!("../fixtures/zalando_postgres_operator_clusterrole.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/clusterrolebinding.yaml",
    expected_fixture: include_str!(
        "../fixtures/zalando_postgres_operator_clusterrolebinding.ir.json"
    ),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const ZALANDO_POSTGRES_OPERATOR_DEPLOYMENT: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/deployment.yaml",
    expected_fixture: include_str!("../fixtures/zalando_postgres_operator_deployment.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

pub const ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS: IrCorpusCase<'static> =
    IrCorpusCase {
        template_path: "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
        expected_fixture: include_str!(
            "../fixtures/zalando_postgres_operator_postgres_pod_priority_class.ir.json"
        ),
        define_sources: test_util::DefineSourceSpec {
            helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
            helper_template_dirs: &[],
            file_sources: &[],
        },
        dump_env: "SYMBOLIC_DUMP",
    };

pub const ZALANDO_POSTGRES_OPERATOR_UI_INGRESS: IrCorpusCase<'static> = IrCorpusCase {
    template_path: "charts/zalando-postgres-operator-ui/templates/ingress.yaml",
    expected_fixture: include_str!("../fixtures/zalando_postgres_operator_ui_ingress.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator-ui/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};
