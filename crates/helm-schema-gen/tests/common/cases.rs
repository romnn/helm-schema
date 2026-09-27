use super::{
    HelmRenderCase, RenderedManifestValidationCase, RenderedSchemaProviderKind, SchemaBehaviorCase,
    SchemaExpectation,
};
use helm_schema_test_support::TemplateId;

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
    schema_case: TemplateId::BitnamiRedisPrometheusrule,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"metrics":{"enabled":true,"prometheusRule":{"enabled":true,"namespace":7}}}"#,
            accepted: true,
            message: "metrics.prometheusRule.namespace is explicitly stringified before rendering",
        },
        SchemaExpectation {
            instance: r#"{"metrics":{"enabled":false,"prometheusRule":{"enabled":true,"namespace":7}}}"#,
            accepted: true,
            message: "PrometheusRule-only namespace should remain unconstrained when metrics disables the resource",
        },
    ],
};

pub const CERT_MANAGER_DEPLOYMENT_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: TemplateId::CertManagerDeployment,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"livenessProbe":{"enabled":true,"failureThreshold":"eight"}}"#,
            accepted: false,
            message: "livenessProbe.failureThreshold must stay integer-like while livenessProbe is enabled",
        },
        SchemaExpectation {
            instance: r#"{"livenessProbe":{"enabled":false,"failureThreshold":"eight"}}"#,
            accepted: true,
            message: "disabled livenessProbe fields should remain unconstrained because the template skips them",
        },
    ],
};

pub const CERT_MANAGER_SERVICE_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: TemplateId::CertManagerService,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"prometheus":{"enabled":true},"serviceAnnotations":{"example.com/bad":7}}"#,
            accepted: false,
            message: "serviceAnnotations must stay a string map when the Service renders",
        },
        SchemaExpectation {
            instance: r#"{"prometheus":{"enabled":false},"serviceAnnotations":{"example.com/bad":7}}"#,
            accepted: true,
            message: "serviceAnnotations should be unconstrained when the Service template is disabled",
        },
    ],
};

pub const NATS_SERVICE_ACCOUNT_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: TemplateId::NatsServiceAccount,
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
    schema_case: TemplateId::NatsService,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"service":{"enabled":true,"name":7}}"#,
            accepted: false,
            message: "service.name must stay string-like when the Service is enabled",
        },
        SchemaExpectation {
            instance: r#"{"nameOverride":7}"#,
            accepted: false,
            message: "nameOverride must stay string-like when the Service is rendered by default",
        },
        SchemaExpectation {
            instance: r#"{"service":{"enabled":false,"name":7}}"#,
            accepted: true,
            message: "service.name should remain unconstrained when the Service is disabled",
        },
        SchemaExpectation {
            instance: r#"{"service":{"enabled":false},"nameOverride":7}"#,
            accepted: false,
            message: "defaultValues computes the fullname before the Service guard and always string-consumes nameOverride",
        },
        SchemaExpectation {
            instance: r#"{"natsBox":{"contexts":{"audit":7}}}"#,
            accepted: false,
            message: "every named NATS context must be an object before defaultValues calls get on it",
        },
        SchemaExpectation {
            instance: r#"{"natsBox":{"contexts":0}}"#,
            accepted: false,
            message: "a JSON-decoded numeric context collection cannot be ranged",
        },
        SchemaExpectation {
            instance: r#"{"natsBox":{"contexts":{"audit":{}}}}"#,
            accepted: true,
            message: "an arbitrary named context may omit all optional secret sections",
        },
        SchemaExpectation {
            instance: r#"{"natsBox":{"contexts":0}}"#,
            accepted: false,
            message: "JSON decoding leaves zero non-iterable for the destructured contexts range",
        },
    ],
};

pub const SIGNOZ_POSTGRESQL_SECRETS_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: TemplateId::SignozPostgresqlSecrets,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"serviceBindings":{"enabled":true},"architecture":"replication","auth":{},"primary":{"name":7}}"#,
            accepted: true,
            message: "primary.name is explicitly stringified by the fullname helper",
        },
        SchemaExpectation {
            instance: r#"{"serviceBindings":{"enabled":false},"architecture":"replication","auth":{},"primary":{"name":7}}"#,
            accepted: true,
            message: "primary.name should remain unconstrained when service bindings are disabled",
        },
    ],
};

pub const SIGNOZ_ZOOKEEPER_STATEFULSET_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: TemplateId::SignozZookeeperStatefulset,
    expectations: &[SchemaExpectation {
        instance: r#"{"containerSecurityContext":{"enabled":true,"runAsUser":"root"}}"#,
        accepted: false,
        message: "containerSecurityContext.runAsUser must stay integer-like while the context is enabled",
    }],
};

pub const SIGNOZ_ZOOKEEPER_SVC_BEHAVIOR: SchemaBehaviorCase<'static> = SchemaBehaviorCase {
    schema_case: TemplateId::SignozZookeeperSvc,
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
        schema_case: TemplateId::ZalandoPostgresOperatorClusterrolebinding,
        expectations: &[
            SchemaExpectation {
                instance: r#"{"rbac":{"create":true},"serviceAccount":{"name":7}}"#,
                accepted: false,
                message: "serviceAccount.name must stay string-like when rbac.create is live",
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
        schema_case: TemplateId::ZalandoPostgresOperatorClusterrole,
        expectations: &[
            SchemaExpectation {
                instance: r#"{"rbac":{"create":true},"serviceAccount":{"name":7}}"#,
                accepted: false,
                message: "serviceAccount.name must stay string-like when rbac.create is live",
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
    schema_case: TemplateId::ZalandoPostgresOperatorPostgresPodPriorityClass,
    expectations: &[
        SchemaExpectation {
            instance: r#"{"podPriorityClassName":{"create":true,"name":7}}"#,
            accepted: false,
            message: "podPriorityClassName.name must stay string-like when create is live",
        },
        SchemaExpectation {
            instance: r#"{"podPriorityClassName":{"create":true,"priority":"high"}}"#,
            accepted: false,
            message: "podPriorityClassName.priority must stay integer-like when create is live",
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
