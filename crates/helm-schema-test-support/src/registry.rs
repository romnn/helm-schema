//! The closed set of corpus artifacts and the exact recipe behind each one.
//!
//! Every fixture the integration suite pins is registered here once: its typed
//! identity, the file name the producer writes, the repository fixture it is
//! compared with, and the complete generation recipe. Two artifacts never share
//! a recipe merely because they read the same chart.

use std::collections::BTreeSet;

use color_eyre::eyre;
use helm_schema::generation::SchemaProfile;
use helm_schema::output::ReferencePolicy;
use indoc::indoc;
use serde_json::{Value, json};

/// Invokes `$callback!` with every chart-corpus case as `(test_name, ChartVariant, "chart-dir")`.
///
/// The registry derives [`ChartId`] from this list, and `chart_corpus.rs` derives
/// its named fixture tests from the same list, so the two can never disagree.
#[macro_export]
macro_rules! corpus_charts {
    ($callback:ident) => {
        $callback! {
            (airflow, Airflow, "airflow"),
            (argo_cd, ArgoCd, "argo-cd"),
            (aws_load_balancer_controller, AwsLoadBalancerController, "aws-load-balancer-controller"),
            (bitnami_postgresql, BitnamiPostgresql, "bitnami-postgresql"),
            (bitnami_redis, BitnamiRedis, "bitnami-redis"),
            (cert_manager, CertManager, "cert-manager"),
            (cilium, Cilium, "cilium"),
            (cloudnative_pg, CloudnativePg, "cloudnative-pg"),
            (cluster_autoscaler, ClusterAutoscaler, "cluster-autoscaler"),
            (common, Common, "common"),
            (coredns, Coredns, "coredns"),
            (crossplane, Crossplane, "crossplane"),
            (datadog, Datadog, "datadog"),
            (dict_config, DictConfig, "dict-config"),
            (external_dns, ExternalDns, "external-dns"),
            (external_secrets, ExternalSecrets, "external-secrets"),
            (falco, Falco, "falco"),
            (fluent_bit, FluentBit, "fluent-bit"),
            (flux2, Flux2, "flux2"),
            (grafana, Grafana, "grafana"),
            (harbor, Harbor, "harbor"),
            (ingress_nginx, IngressNginx, "ingress-nginx"),
            (istiod, Istiod, "istiod"),
            (jaeger, Jaeger, "jaeger"),
            (jenkins, Jenkins, "jenkins"),
            (karpenter, Karpenter, "karpenter"),
            (keda, Keda, "keda"),
            (kube_prometheus_stack, KubePrometheusStack, "kube-prometheus-stack"),
            (kube_state_metrics, KubeStateMetrics, "kube-state-metrics"),
            (kyverno, Kyverno, "kyverno"),
            (loki, Loki, "loki"),
            (longhorn, Longhorn, "longhorn"),
            (metallb, Metallb, "metallb"),
            (metrics_server, MetricsServer, "metrics-server"),
            (minio, Minio, "minio"),
            (nack, Nack, "nack"),
            (nats, Nats, "nats"),
            (nats_account_server, NatsAccountServer, "nats-account-server"),
            (nats_kafka, NatsKafka, "nats-kafka"),
            (nats_operator, NatsOperator, "nats-operator"),
            (nfs_subdir_external_provisioner, NfsSubdirExternalProvisioner, "nfs-subdir-external-provisioner"),
            (oauth2_proxy, Oauth2Proxy, "oauth2-proxy"),
            (prometheus, Prometheus, "prometheus"),
            (promtail, Promtail, "promtail"),
            (reloader, Reloader, "reloader"),
            (schema_emission_unconditional_fail, SchemaEmissionUnconditionalFail, "schema-emission-unconditional-fail"),
            (sealed_secrets, SealedSecrets, "sealed-secrets"),
            (signoz_signoz, SignozSignoz, "signoz-signoz"),
            (surveyor, Surveyor, "surveyor"),
            (tempo, Tempo, "tempo"),
            (traefik, Traefik, "traefik"),
            (trivy_operator, TrivyOperator, "trivy-operator"),
            (vault, Vault, "vault"),
            (velero, Velero, "velero"),
            (zalando_postgres_operator, ZalandoPostgresOperator, "zalando-postgres-operator"),
            (zalando_postgres_operator_ui, ZalandoPostgresOperatorUi, "zalando-postgres-operator-ui"),
            // Corpus-expansion-v1 intake, unadjudicated.
            (aws_ebs_csi_driver, AwsEbsCsiDriver, "aws-ebs-csi-driver"),
            (dify, Dify, "dify"),
            (eck_stack, EckStack, "eck-stack"),
            (gitea, Gitea, "gitea"),
            (graylog, Graylog, "graylog"),
            (headscale, Headscale, "headscale"),
            (imgproxy, Imgproxy, "imgproxy"),
            (kube_starrocks, KubeStarrocks, "kube-starrocks"),
            (kubeshark, Kubeshark, "kubeshark"),
            (milvus, Milvus, "milvus"),
            (nacos, Nacos, "nacos"),
            (netbox, Netbox, "netbox"),
            (nginx_ingress, NginxIngress, "nginx-ingress"),
            (okteto, Okteto, "okteto"),
            (oncall, Oncall, "oncall"),
            (openebs, Openebs, "openebs"),
            (openldap_stack_ha, OpenldapStackHa, "openldap-stack-ha"),
            (redmine, Redmine, "redmine"),
            (schema_registry, SchemaRegistry, "schema-registry"),
            (spinnaker, Spinnaker, "spinnaker"),
            (stacks_blockchain_api, StacksBlockchainApi, "stacks-blockchain-api"),
            (synapse, Synapse, "synapse"),
            (weblate, Weblate, "weblate"),
            (yourls, Yourls, "yourls"),
            (actions_runner_controller, ActionsRunnerController, "actions-runner-controller"),
            (alertmanager, Alertmanager, "alertmanager"),
            (alloy, Alloy, "alloy"),
            (apisix, Apisix, "apisix"),
            (argo_events, ArgoEvents, "argo-events"),
            (argo_rollouts, ArgoRollouts, "argo-rollouts"),
            (argo_workflows, ArgoWorkflows, "argo-workflows"),
            (argocd_apps, ArgocdApps, "argocd-apps"),
            (argocd_image_updater, ArgocdImageUpdater, "argocd-image-updater"),
            (aws_for_fluent_bit, AwsForFluentBit, "aws-for-fluent-bit"),
            (aws_node_termination_handler, AwsNodeTerminationHandler, "aws-node-termination-handler"),
            (base, Base, "base"),
            (chartmuseum, Chartmuseum, "chartmuseum"),
            (clickhouse, Clickhouse, "clickhouse"),
            (consul, Consul, "consul"),
            (descheduler, Descheduler, "descheduler"),
            (dex, Dex, "dex"),
            (eck_operator, EckOperator, "eck-operator"),
            (elasticsearch, Elasticsearch, "elasticsearch"),
            (etcd, Etcd, "etcd"),
            (filebeat, Filebeat, "filebeat"),
            (fluentd, Fluentd, "fluentd"),
            (gateway, Gateway, "gateway"),
            (gitlab_runner, GitlabRunner, "gitlab-runner"),
            (goldilocks, Goldilocks, "goldilocks"),
            (headlamp, Headlamp, "headlamp"),
            (home_assistant, HomeAssistant, "home-assistant"),
            (influxdb, Influxdb, "influxdb"),
            (jira, Jira, "jira"),
            (jupyterhub, Jupyterhub, "jupyterhub"),
            (kafka_ui, KafkaUi, "kafka-ui"),
            (keycloakx, Keycloakx, "keycloakx"),
            (kibana, Kibana, "kibana"),
            (kubernetes_event_exporter, KubernetesEventExporter, "kubernetes-event-exporter"),
            (kubeview, Kubeview, "kubeview"),
            (kured, Kured, "kured"),
            (logstash, Logstash, "logstash"),
            (mailhog, Mailhog, "mailhog"),
            (mariadb, Mariadb, "mariadb"),
            (mariadb_galera, MariadbGalera, "mariadb-galera"),
            (metabase, Metabase, "metabase"),
            (minecraft, Minecraft, "minecraft"),
            (nfs_server_provisioner, NfsServerProvisioner, "nfs-server-provisioner"),
            (nginx, Nginx, "nginx"),
            (nginx_ingress_controller, NginxIngressController, "nginx-ingress-controller"),
            (ollama, Ollama, "ollama"),
            (open_webui, OpenWebui, "open-webui"),
            (opensearch, Opensearch, "opensearch"),
            (opentelemetry_operator, OpentelemetryOperator, "opentelemetry-operator"),
            (pgadmin4, Pgadmin4, "pgadmin4"),
            (phpmyadmin, Phpmyadmin, "phpmyadmin"),
            (pihole, Pihole, "pihole"),
            (postgresql_ha, PostgresqlHa, "postgresql-ha"),
            (prometheus_adapter, PrometheusAdapter, "prometheus-adapter"),
            (prometheus_blackbox_exporter, PrometheusBlackboxExporter, "prometheus-blackbox-exporter"),
            (prometheus_node_exporter, PrometheusNodeExporter, "prometheus-node-exporter"),
            (prometheus_operator_crds, PrometheusOperatorCrds, "prometheus-operator-crds"),
            (prometheus_pushgateway, PrometheusPushgateway, "prometheus-pushgateway"),
            (prometheus_redis_exporter, PrometheusRedisExporter, "prometheus-redis-exporter"),
            (qdrant, Qdrant, "qdrant"),
            (rabbitmq, Rabbitmq, "rabbitmq"),
            (rabbitmq_cluster_operator, RabbitmqClusterOperator, "rabbitmq-cluster-operator"),
            (rancher, Rancher, "rancher"),
            (redis_cluster, RedisCluster, "redis-cluster"),
            (redis_ha, RedisHa, "redis-ha"),
            (rook_ceph, RookCeph, "rook-ceph"),
            (spark, Spark, "spark"),
            (terraform, Terraform, "terraform"),
            (tigera_operator, TigeraOperator, "tigera-operator"),
            (trino, Trino, "trino"),
            (uptime_kuma, UptimeKuma, "uptime-kuma"),
            (vector, Vector, "vector"),
            (vpa, Vpa, "vpa"),
            (x509_certificate_exporter, X509CertificateExporter, "x509-certificate-exporter"),
            (zabbix, Zabbix, "zabbix"),
            (zookeeper, Zookeeper, "zookeeper"),
        }
    };
}

macro_rules! define_chart_ids {
    ($(($test:ident, $variant:ident, $chart:literal)),* $(,)?) => {
        /// A chart under `testdata/charts` whose whole-chart schema the registry produces.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum ChartId {
            $(
                #[doc = concat!("The `", $chart, "` corpus chart.")]
                $variant,
            )*
            /// The `PostgreSQL` chart vendored inside `signoz-signoz`.
            ///
            /// It has no chart-corpus fixture; its schema is an internal
            /// artifact read by `chart_signoz_postgresql.rs`.
            SignozPostgresql,
        }

        impl ChartId {
            /// Every chart with a committed chart-corpus fixture, in roster order.
            pub const CORPUS: &[ChartId] = &[$(ChartId::$variant),*];

            /// The chart directory relative to `testdata/charts`.
            #[must_use]
            pub fn relative_path(self) -> &'static str {
                match self {
                    $(ChartId::$variant => $chart,)*
                    ChartId::SignozPostgresql => {
                        "signoz-signoz/charts/signoz-otel-gateway/charts/postgresql"
                    }
                }
            }
        }
    };
}

corpus_charts!(define_chart_ids);

impl ChartId {
    /// Looks up the registered chart whose directory is `relative_path`.
    #[must_use]
    pub fn from_relative_path(relative_path: &str) -> Option<ChartId> {
        ChartId::CORPUS
            .iter()
            .copied()
            .chain([ChartId::SignozPostgresql])
            .find(|chart| chart.relative_path() == relative_path)
    }
}

/// One template-level schema case of the generator corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemplateId {
    /// `bitnami-redis` network policy.
    BitnamiRedisNetworkpolicy,
    /// `bitnami-redis` Prometheus rule.
    BitnamiRedisPrometheusrule,
    /// `dict-config` pod disruption budget.
    DictConfigPdb,
    /// `dict-config` ingress.
    DictConfigIngress,
    /// `cert-manager` deployment.
    CertManagerDeployment,
    /// `cert-manager` service.
    CertManagerService,
    /// `nats-operator` RBAC.
    NatsOperatorRbac,
    /// `nats` service account.
    NatsServiceAccount,
    /// `nats` service.
    NatsService,
    /// Nested `SigNoz` `PostgreSQL` secrets.
    SignozPostgresqlSecrets,
    /// Nested `SigNoz` `ZooKeeper` stateful set.
    SignozZookeeperStatefulset,
    /// Nested `SigNoz` `ZooKeeper` service.
    SignozZookeeperSvc,
    /// `surveyor` config map, generated over inline values.
    SurveyorConfigmap,
    /// `surveyor` horizontal pod autoscaler.
    SurveyorHpa,
    /// `surveyor` service monitor.
    SurveyorServiceMonitor,
    /// `zalando-postgres-operator` cluster role binding.
    ZalandoPostgresOperatorClusterrolebinding,
    /// `zalando-postgres-operator` cluster role.
    ZalandoPostgresOperatorClusterrole,
    /// `zalando-postgres-operator` deployment.
    ZalandoPostgresOperatorDeployment,
    /// `zalando-postgres-operator-ui` ingress.
    ZalandoPostgresOperatorUiIngress,
    /// `zalando-postgres-operator` pod priority class.
    ZalandoPostgresOperatorPostgresPodPriorityClass,
}

impl TemplateId {
    /// Every template case, in fixture-lane order.
    pub const ALL: &[TemplateId] = &[
        TemplateId::BitnamiRedisNetworkpolicy,
        TemplateId::BitnamiRedisPrometheusrule,
        TemplateId::DictConfigPdb,
        TemplateId::DictConfigIngress,
        TemplateId::CertManagerDeployment,
        TemplateId::CertManagerService,
        TemplateId::NatsOperatorRbac,
        TemplateId::NatsServiceAccount,
        TemplateId::NatsService,
        TemplateId::SignozPostgresqlSecrets,
        TemplateId::SignozZookeeperStatefulset,
        TemplateId::SignozZookeeperSvc,
        TemplateId::SurveyorConfigmap,
        TemplateId::SurveyorHpa,
        TemplateId::SurveyorServiceMonitor,
        TemplateId::ZalandoPostgresOperatorClusterrolebinding,
        TemplateId::ZalandoPostgresOperatorClusterrole,
        TemplateId::ZalandoPostgresOperatorDeployment,
        TemplateId::ZalandoPostgresOperatorUiIngress,
        TemplateId::ZalandoPostgresOperatorPostgresPodPriorityClass,
    ];
}

/// One lean-profile chart fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LeanId {
    /// `schema-emission-controls`.
    Controls,
    /// `schema-emission-local-kind`.
    LocalKind,
    /// `schema-emission-temporal-wrapper`.
    TemporalWrapper,
    /// `schema-emission-unconditional-fail`.
    UnconditionalFail,
}

impl LeanId {
    /// Every lean fixture.
    pub const ALL: &[LeanId] = &[
        LeanId::Controls,
        LeanId::LocalKind,
        LeanId::TemporalWrapper,
        LeanId::UnconditionalFail,
    ];

    /// The chart directory relative to `testdata/charts`.
    #[must_use]
    pub fn chart(self) -> &'static str {
        match self {
            LeanId::Controls => "schema-emission-controls",
            LeanId::LocalKind => "schema-emission-local-kind",
            LeanId::TemporalWrapper => "schema-emission-temporal-wrapper",
            LeanId::UnconditionalFail => "schema-emission-unconditional-fail",
        }
    }
}

/// One final-output policy fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PolicyId {
    /// Full profile, self-contained references.
    Full,
    /// Lean profile, self-contained references.
    Lean,
    /// Full profile with a caller override that tries to forge policy annotations.
    CallerOverwrite,
    /// Full profile with a Boolean `false` override root.
    BooleanFalse,
}

impl PolicyId {
    /// Every final-output policy fixture.
    pub const ALL: &[PolicyId] = &[
        PolicyId::Full,
        PolicyId::Lean,
        PolicyId::CallerOverwrite,
        PolicyId::BooleanFalse,
    ];

    /// The fixture name under `testdata/final-output-schemas`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            PolicyId::Full => "full",
            PolicyId::Lean => "lean",
            PolicyId::CallerOverwrite => "caller-overwrite",
            PolicyId::BooleanFalse => "boolean-false",
        }
    }

    /// The complete generation recipe of this fixture.
    #[must_use]
    pub fn recipe(self) -> PolicyRecipe {
        let full = ChartRecipe::corpus(POLICY_CHART, SchemaProfile::Full);
        match self {
            PolicyId::Full => PolicyRecipe {
                chart: full,
                reference_policy: ReferencePolicy::SelfContained,
                override_file: None,
            },
            PolicyId::Lean => PolicyRecipe {
                chart: ChartRecipe::corpus(POLICY_CHART, SchemaProfile::Lean),
                reference_policy: ReferencePolicy::SelfContained,
                override_file: None,
            },
            PolicyId::CallerOverwrite => PolicyRecipe {
                chart: full,
                reference_policy: ReferencePolicy::PreserveRefs,
                override_file: Some(OverrideFile {
                    name: "caller.json",
                    contents: r#"{"description":"caller override","x-helm-schema-generated":false,"x-helm-schema-policy":{"forged":true}}"#,
                }),
            },
            PolicyId::BooleanFalse => PolicyRecipe {
                chart: full,
                reference_policy: ReferencePolicy::SelfContained,
                override_file: Some(OverrideFile {
                    name: "boolean.json",
                    contents: "false\n",
                }),
            },
        }
    }
}

/// The chart every final-output policy fixture is generated from.
pub const POLICY_CHART: &str = "schema-emission-controls";

/// One symbolic-IR fixture of the IR corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IrId {
    /// `bitnami-redis` network policy.
    BitnamiRedisNetworkpolicy,
    /// `bitnami-redis` Prometheus rule.
    BitnamiRedisPrometheusrule,
    /// `cert-manager` deployment.
    CertManagerDeployment,
    /// `cert-manager` service.
    CertManagerService,
    /// `nats-operator` RBAC.
    NatsOperatorRbac,
    /// `nats` service.
    NatsService,
    /// `nats` service account.
    NatsServiceAccount,
    /// Nested `SigNoz` `PostgreSQL` secrets.
    SignozPostgresqlSecrets,
    /// Nested `SigNoz` `ZooKeeper` stateful set.
    SignozZookeeperStatefulset,
    /// Nested `SigNoz` `ZooKeeper` service.
    SignozZookeeperSvc,
    /// `surveyor` config map.
    SurveyorConfigmap,
    /// `surveyor` horizontal pod autoscaler.
    SurveyorHpa,
    /// `surveyor` service monitor.
    SurveyorServiceMonitor,
    /// `zalando-postgres-operator` cluster role.
    ZalandoPostgresOperatorClusterrole,
    /// `zalando-postgres-operator` cluster role binding.
    ZalandoPostgresOperatorClusterrolebinding,
    /// `zalando-postgres-operator` deployment.
    ZalandoPostgresOperatorDeployment,
    /// `zalando-postgres-operator` pod priority class.
    ZalandoPostgresOperatorPostgresPodPriorityClass,
    /// `zalando-postgres-operator-ui` ingress.
    ZalandoPostgresOperatorUiIngress,
}

impl IrId {
    /// Every IR case, in fixture-lane order.
    pub const ALL: &[IrId] = &[
        IrId::BitnamiRedisNetworkpolicy,
        IrId::BitnamiRedisPrometheusrule,
        IrId::CertManagerDeployment,
        IrId::CertManagerService,
        IrId::NatsOperatorRbac,
        IrId::NatsService,
        IrId::NatsServiceAccount,
        IrId::SignozPostgresqlSecrets,
        IrId::SignozZookeeperStatefulset,
        IrId::SignozZookeeperSvc,
        IrId::SurveyorConfigmap,
        IrId::SurveyorHpa,
        IrId::SurveyorServiceMonitor,
        IrId::ZalandoPostgresOperatorClusterrole,
        IrId::ZalandoPostgresOperatorClusterrolebinding,
        IrId::ZalandoPostgresOperatorDeployment,
        IrId::ZalandoPostgresOperatorPostgresPodPriorityClass,
        IrId::ZalandoPostgresOperatorUiIngress,
    ];
}

/// The closed identity of every artifact the producer writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArtifactId {
    /// A full-profile whole-chart schema.
    Chart(ChartId),
    /// A template-level generator schema.
    Template(TemplateId),
    /// A lean-profile whole-chart schema.
    Lean(LeanId),
    /// A final-output policy schema.
    FinalPolicy(PolicyId),
    /// A symbolic contract-IR document.
    Ir(IrId),
}

/// The artifact families, used to check the registry's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArtifactKind {
    /// [`ArtifactId::Chart`].
    Chart,
    /// [`ArtifactId::Template`].
    Template,
    /// [`ArtifactId::Lean`].
    Lean,
    /// [`ArtifactId::FinalPolicy`].
    FinalPolicy,
    /// [`ArtifactId::Ir`].
    Ir,
}

impl ArtifactKind {
    /// The stable lowercase name recorded in the manifest.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ArtifactKind::Chart => "chart",
            ArtifactKind::Template => "template",
            ArtifactKind::Lean => "lean",
            ArtifactKind::FinalPolicy => "final-policy",
            ArtifactKind::Ir => "ir",
        }
    }
}

/// Where a produced artifact is compared or used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactTarget {
    /// A committed fixture, as a repository-relative path.
    Fixture(String),
    /// An artifact read only by semantic tests; it has no committed fixture
    /// and lives under the producer's `internal/` directory.
    Internal,
}

/// One registered artifact: identity, produced file name, target and recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSpec {
    /// Typed identity.
    pub id: ArtifactId,
    /// The file name the producer writes; equal to the historical dump name.
    pub dump_name: String,
    /// The committed fixture this artifact must equal, if any.
    pub target: ArtifactTarget,
    /// Everything generation reads besides the checked-out source tree.
    pub recipe: GenerationRecipe,
}

/// A complete generation recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationRecipe {
    /// Whole-chart generation through an analysis session.
    Chart(ChartRecipe),
    /// Template-level generation through the generator API.
    Template(TemplateRecipe),
    /// Whole-chart generation followed by the final-output pipeline.
    FinalPolicy(PolicyRecipe),
    /// Symbolic contract-IR extraction for one template.
    Ir(IrRecipe),
}

/// Whole-chart generation options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartRecipe {
    /// Chart directory relative to `testdata/charts`.
    pub chart: &'static str,
    /// Emission profile.
    pub profile: SchemaProfile,
    /// Whether `templates/tests` are analyzed.
    pub include_tests: bool,
    /// Whether dependency values are exposed beneath their subchart keys.
    pub include_subchart_values: bool,
    /// Extra values files relative to `testdata`.
    pub values_files: &'static [&'static str],
    /// Whether the required-property heuristic runs.
    pub infer_required: bool,
    /// Primary Kubernetes version of the vendored provider bundle.
    pub k8s_version: &'static str,
    /// Whether repeated subtrees are interned into root `$defs`.
    pub minimize: bool,
}

impl ChartRecipe {
    /// The chart-corpus generation options: offline vendored provider bundle,
    /// subchart values included, tests excluded, minimized like the CLI output.
    #[must_use]
    pub fn corpus(chart: &'static str, profile: SchemaProfile) -> ChartRecipe {
        ChartRecipe {
            chart,
            profile,
            // Helm RENDERS `templates/tests/*` and only filters them out of the
            // output afterwards, so those roots ARE real contracts and the
            // shipped default includes them. Switching the corpus over waits on
            // the provider-required-leaf family: kyverno's
            // `test.nodeSelector`/`tolerations` accept values that render but
            // whose manifest strict validation rejects, which is a claim about
            // the rendered sink, not about the guards the test template adds.
            // Tracked in `plan/chart-corpus-status.md`.
            include_tests: false,
            include_subchart_values: true,
            values_files: &[],
            infer_required: false,
            k8s_version: BUNDLE_K8S_VERSION,
            minimize: true,
        }
    }
}

/// The Kubernetes version whole-chart recipes read from the vendored bundle.
pub const BUNDLE_K8S_VERSION: &str = "v1.29.0-standalone-strict";

/// Final-output policy options layered over a whole-chart recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyRecipe {
    /// The analysis session's generation options.
    pub chart: ChartRecipe,
    /// Reference handling of the final output.
    pub reference_policy: ReferencePolicy,
    /// Caller override schema written to a scratch file before emission.
    pub override_file: Option<OverrideFile>,
}

/// A caller override schema file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverrideFile {
    /// File name inside the scratch directory.
    pub name: &'static str,
    /// Exact file bytes.
    pub contents: &'static str,
}

/// Template-level generator inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateRecipe {
    /// Template file relative to `testdata`.
    pub template_path: &'static str,
    /// Chart values file relative to `testdata`.
    pub values_path: &'static str,
    /// Values document used instead of `values_path` when present.
    pub inline_values: Option<&'static str>,
    /// Helper sources indexed before extraction.
    pub define_sources: test_util::DefineSourceSpec<'static>,
    /// Resource-schema provider chain.
    pub provider: TemplateProvider,
}

/// The provider chain of a template-level recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateProvider {
    /// Vendored Kubernetes schemas at this version.
    K8s(&'static str),
    /// Vendored CRD catalog, then Kubernetes schemas at this version.
    CrdK8s(&'static str),
}

/// Symbolic-IR extraction inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrRecipe {
    /// Template file relative to `testdata`.
    pub template_path: &'static str,
    /// Helper sources indexed before extraction.
    pub define_sources: test_util::DefineSourceSpec<'static>,
}

/// Registered naming and inputs of one template case.
#[derive(Debug, Clone, Copy)]
pub struct TemplateCase {
    /// Historical dump stem: the artifact is `helm-schema.<stem>.schema.json`.
    pub dump_stem: &'static str,
    /// Fixture file name under `crates/helm-schema-gen/tests/fixtures`.
    pub fixture: &'static str,
    /// Generation inputs.
    pub recipe: TemplateRecipe,
}

/// Registered naming and inputs of one IR case.
#[derive(Debug, Clone, Copy)]
pub struct IrCase {
    /// Fixture file name under `crates/helm-schema-ir/tests/fixtures`.
    pub fixture: &'static str,
    /// Extraction inputs.
    pub recipe: IrRecipe,
}

const SURVEYOR_CONFIGMAP_VALUES: &str = indoc! {r#"
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
"#};

impl TemplateId {
    /// The registered naming and inputs of this case.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "the twenty template cases read best as one flat table"
    )]
    pub fn case(self) -> TemplateCase {
        match self {
            TemplateId::BitnamiRedisNetworkpolicy => TemplateCase {
                dump_stem: "bitnami-redis.networkpolicy",
                fixture: "bitnami_redis_networkpolicy.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/bitnami-redis/templates/networkpolicy.yaml",
                    values_path: "charts/bitnami-redis/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
                        helper_template_dirs: &[("charts/common/templates", "tpl")],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::BitnamiRedisPrometheusrule => TemplateCase {
                dump_stem: "bitnami-redis.prometheusrule",
                fixture: "bitnami_redis_prometheusrule.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/bitnami-redis/templates/prometheusrule.yaml",
                    values_path: "charts/bitnami-redis/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
                        helper_template_dirs: &[("charts/common/templates", "tpl")],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::CrdK8s("v1.35.0"),
                },
            },
            TemplateId::DictConfigPdb => TemplateCase {
                dump_stem: "dict-config.pdb",
                fixture: "dict_config_pdb.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/dict-config/templates/pdb.yaml",
                    values_path: "charts/dict-config/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/dict-config/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::DictConfigIngress => TemplateCase {
                dump_stem: "dict-config.ingress",
                fixture: "dict_config_ingress.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/dict-config/templates/ingress.yaml",
                    values_path: "charts/dict-config/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/dict-config/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::CertManagerDeployment => TemplateCase {
                dump_stem: "cert-manager.deployment",
                fixture: "cert_manager_deployment.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/cert-manager/templates/deployment.yaml",
                    values_path: "charts/cert-manager/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::CertManagerService => TemplateCase {
                dump_stem: "cert-manager.service",
                fixture: "cert_manager_service.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/cert-manager/templates/service.yaml",
                    values_path: "charts/cert-manager/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::NatsOperatorRbac => TemplateCase {
                dump_stem: "nats-operator.rbac",
                fixture: "nats_operator_rbac.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/nats-operator/templates/rbac.yaml",
                    values_path: "charts/nats-operator/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/nats-operator/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::NatsServiceAccount => TemplateCase {
                dump_stem: "nats-service-account",
                fixture: "nats_service_account.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/nats/templates/service-account.yaml",
                    values_path: "charts/nats/values.yaml",
                    inline_values: None,
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
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::NatsService => TemplateCase {
                dump_stem: "nats-service",
                fixture: "nats_service.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/nats/templates/service.yaml",
                    values_path: "charts/nats/values.yaml",
                    inline_values: None,
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
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::SignozPostgresqlSecrets => TemplateCase {
                dump_stem: "signoz-postgresql-secrets",
                fixture: "signoz_postgresql_secrets.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml",
                    values_path: "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/values.yaml",
                    inline_values: None,
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
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::SignozZookeeperStatefulset => TemplateCase {
                dump_stem: "signoz-zookeeper-statefulset",
                fixture: "signoz_zookeeper_statefulset.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml",
                    values_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/values.yaml",
                    inline_values: None,
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
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::SignozZookeeperSvc => TemplateCase {
                dump_stem: "signoz-zookeeper-svc",
                fixture: "signoz_zookeeper_svc.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml",
                    values_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/values.yaml",
                    inline_values: None,
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
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::SurveyorConfigmap => TemplateCase {
                dump_stem: "surveyor.configmap",
                fixture: "surveyor_configmap.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/surveyor/templates/configmap.yaml",
                    values_path: "charts/surveyor/values.yaml",
                    inline_values: Some(SURVEYOR_CONFIGMAP_VALUES),
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::SurveyorHpa => TemplateCase {
                dump_stem: "surveyor.hpa",
                fixture: "surveyor_hpa.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/surveyor/templates/hpa.yaml",
                    values_path: "charts/surveyor/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.24.0"),
                },
            },
            TemplateId::SurveyorServiceMonitor => TemplateCase {
                dump_stem: "surveyor.service-monitor",
                fixture: "surveyor_service_monitor.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/surveyor/templates/serviceMonitor.yaml",
                    values_path: "charts/surveyor/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::CrdK8s("v1.35.0"),
                },
            },
            TemplateId::ZalandoPostgresOperatorClusterrolebinding => TemplateCase {
                dump_stem: "zalando-postgres-operator.clusterrolebinding",
                fixture: "zalando_postgres_operator_clusterrolebinding.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/zalando-postgres-operator/templates/clusterrolebinding.yaml",
                    values_path: "charts/zalando-postgres-operator/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::ZalandoPostgresOperatorClusterrole => TemplateCase {
                dump_stem: "zalando-postgres-operator.clusterrole",
                fixture: "zalando_postgres_operator_clusterrole.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/zalando-postgres-operator/templates/clusterrole.yaml",
                    values_path: "charts/zalando-postgres-operator/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::ZalandoPostgresOperatorDeployment => TemplateCase {
                dump_stem: "zalando-postgres-operator.deployment",
                fixture: "zalando_postgres_operator_deployment.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/zalando-postgres-operator/templates/deployment.yaml",
                    values_path: "charts/zalando-postgres-operator/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::ZalandoPostgresOperatorUiIngress => TemplateCase {
                dump_stem: "zalando-postgres-operator-ui.ingress",
                fixture: "zalando_postgres_operator_ui_ingress.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/zalando-postgres-operator-ui/templates/ingress.yaml",
                    values_path: "charts/zalando-postgres-operator-ui/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator-ui/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
            TemplateId::ZalandoPostgresOperatorPostgresPodPriorityClass => TemplateCase {
                dump_stem: "zalando-postgres-operator.postgres-pod-priority-class",
                fixture: "zalando_postgres_operator_postgres_pod_priority_class.schema.json",
                recipe: TemplateRecipe {
                    template_path: "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
                    values_path: "charts/zalando-postgres-operator/values.yaml",
                    inline_values: None,
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                    provider: TemplateProvider::K8s("v1.35.0"),
                },
            },
        }
    }
}

impl IrId {
    /// The registered naming and inputs of this case.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "the eighteen IR cases read best as one flat table"
    )]
    pub fn case(self) -> IrCase {
        match self {
            IrId::BitnamiRedisNetworkpolicy => IrCase {
                fixture: "bitnami_redis_networkpolicy.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/bitnami-redis/templates/networkpolicy.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
                        helper_template_dirs: &[("charts/common/templates", "tpl")],
                        file_sources: &[],
                    },
                },
            },
            IrId::BitnamiRedisPrometheusrule => IrCase {
                fixture: "bitnami_redis_prometheusrule.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/bitnami-redis/templates/prometheusrule.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
                        helper_template_dirs: &[("charts/common/templates", "tpl")],
                        file_sources: &[],
                    },
                },
            },
            IrId::CertManagerDeployment => IrCase {
                fixture: "cert_manager_deployment.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/cert-manager/templates/deployment.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::CertManagerService => IrCase {
                fixture: "cert_manager_service.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/cert-manager/templates/service.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/cert-manager/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::NatsOperatorRbac => IrCase {
                fixture: "nats_operator_rbac.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/nats-operator/templates/rbac.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/nats-operator/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::NatsService => IrCase {
                fixture: "nats_service.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/nats/templates/service.yaml",
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
                },
            },
            IrId::NatsServiceAccount => IrCase {
                fixture: "nats_service_account.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/nats/templates/service-account.yaml",
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
                },
            },
            IrId::SignozPostgresqlSecrets => IrCase {
                fixture: "signoz_postgresql_secrets.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml",
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
                },
            },
            IrId::SignozZookeeperStatefulset => IrCase {
                fixture: "signoz_zookeeper_statefulset.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml",
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
                },
            },
            IrId::SignozZookeeperSvc => IrCase {
                fixture: "signoz_zookeeper_svc.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml",
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
                },
            },
            IrId::SurveyorConfigmap => IrCase {
                fixture: "surveyor_configmap.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/surveyor/templates/configmap.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::SurveyorHpa => IrCase {
                fixture: "surveyor_hpa.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/surveyor/templates/hpa.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::SurveyorServiceMonitor => IrCase {
                fixture: "surveyor_service_monitor.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/surveyor/templates/serviceMonitor.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::ZalandoPostgresOperatorClusterrole => IrCase {
                fixture: "zalando_postgres_operator_clusterrole.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/zalando-postgres-operator/templates/clusterrole.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::ZalandoPostgresOperatorClusterrolebinding => IrCase {
                fixture: "zalando_postgres_operator_clusterrolebinding.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/zalando-postgres-operator/templates/clusterrolebinding.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::ZalandoPostgresOperatorDeployment => IrCase {
                fixture: "zalando_postgres_operator_deployment.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/zalando-postgres-operator/templates/deployment.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::ZalandoPostgresOperatorPostgresPodPriorityClass => IrCase {
                fixture: "zalando_postgres_operator_postgres_pod_priority_class.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
            IrId::ZalandoPostgresOperatorUiIngress => IrCase {
                fixture: "zalando_postgres_operator_ui_ingress.ir.json",
                recipe: IrRecipe {
                    template_path: "charts/zalando-postgres-operator-ui/templates/ingress.yaml",
                    define_sources: test_util::DefineSourceSpec {
                        helper_templates: &[
                            "charts/zalando-postgres-operator-ui/templates/_helpers.tpl",
                        ],
                        helper_template_dirs: &[],
                        file_sources: &[],
                    },
                },
            },
        }
    }
}

impl ArtifactId {
    /// The family this artifact belongs to.
    #[must_use]
    pub fn kind(self) -> ArtifactKind {
        match self {
            ArtifactId::Chart(_) => ArtifactKind::Chart,
            ArtifactId::Template(_) => ArtifactKind::Template,
            ArtifactId::Lean(_) => ArtifactKind::Lean,
            ArtifactId::FinalPolicy(_) => ArtifactKind::FinalPolicy,
            ArtifactId::Ir(_) => ArtifactKind::Ir,
        }
    }

    /// The registered file name, target and recipe of this artifact.
    #[must_use]
    pub fn spec(self) -> ArtifactSpec {
        match self {
            ArtifactId::Chart(ChartId::SignozPostgresql) => ArtifactSpec {
                id: self,
                dump_name: "helm-schema.cli.chart-internal.signoz-postgresql.schema.json"
                    .to_string(),
                target: ArtifactTarget::Internal,
                recipe: GenerationRecipe::Chart(ChartRecipe::corpus(
                    ChartId::SignozPostgresql.relative_path(),
                    SchemaProfile::Full,
                )),
            },
            ArtifactId::Chart(chart) => {
                let name = chart.relative_path();
                ArtifactSpec {
                    id: self,
                    dump_name: format!("helm-schema.cli.chart-corpus.{name}.schema.json"),
                    target: ArtifactTarget::Fixture(format!(
                        "testdata/chart-corpus-schemas/{name}.schema.json"
                    )),
                    recipe: GenerationRecipe::Chart(ChartRecipe::corpus(name, SchemaProfile::Full)),
                }
            }
            ArtifactId::Template(template) => {
                let case = template.case();
                ArtifactSpec {
                    id: self,
                    dump_name: format!("helm-schema.{}.schema.json", case.dump_stem),
                    target: ArtifactTarget::Fixture(format!(
                        "crates/helm-schema-gen/tests/fixtures/{}",
                        case.fixture
                    )),
                    recipe: GenerationRecipe::Template(case.recipe),
                }
            }
            ArtifactId::Lean(lean) => {
                let name = lean.chart();
                ArtifactSpec {
                    id: self,
                    dump_name: format!("helm-schema.emission-profile.lean.{name}.schema.json"),
                    target: ArtifactTarget::Fixture(format!(
                        "testdata/emission-profile-schemas/lean/{name}.schema.json"
                    )),
                    recipe: GenerationRecipe::Chart(ChartRecipe::corpus(name, SchemaProfile::Lean)),
                }
            }
            ArtifactId::FinalPolicy(policy) => {
                let name = policy.name();
                ArtifactSpec {
                    id: self,
                    dump_name: format!("helm-schema.final-output.{name}.schema.json"),
                    target: ArtifactTarget::Fixture(format!(
                        "testdata/final-output-schemas/{name}.schema.json"
                    )),
                    recipe: GenerationRecipe::FinalPolicy(policy.recipe()),
                }
            }
            ArtifactId::Ir(ir) => {
                let case = ir.case();
                // The historical IR dump stem is the template path with every
                // non-alphanumeric byte replaced by `-`.
                let stem = case
                    .recipe
                    .template_path
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .collect::<String>();
                ArtifactSpec {
                    id: self,
                    dump_name: format!("helm-schema-ir.{stem}.ir.json"),
                    target: ArtifactTarget::Fixture(format!(
                        "crates/helm-schema-ir/tests/fixtures/{}",
                        case.fixture
                    )),
                    recipe: GenerationRecipe::Ir(case.recipe),
                }
            }
        }
    }

    /// A stable, unique textual key for the manifest.
    #[must_use]
    pub fn key(self) -> String {
        let kind = self.kind().name();
        match self {
            ArtifactId::Chart(chart) => format!("{kind}/{}", chart.relative_path()),
            ArtifactId::Template(template) => format!("{kind}/{}", template.case().dump_stem),
            ArtifactId::Lean(lean) => format!("{kind}/{}", lean.chart()),
            ArtifactId::FinalPolicy(policy) => format!("{kind}/{}", policy.name()),
            ArtifactId::Ir(ir) => format!("{kind}/{}", ir.case().recipe.template_path),
        }
    }
}

/// Every registered artifact, in a stable order.
#[must_use]
pub fn registry() -> Vec<ArtifactSpec> {
    let mut ids = Vec::new();
    for chart in ChartId::CORPUS {
        ids.push(ArtifactId::Chart(*chart));
    }
    ids.push(ArtifactId::Chart(ChartId::SignozPostgresql));
    for template in TemplateId::ALL {
        ids.push(ArtifactId::Template(*template));
    }
    for lean in LeanId::ALL {
        ids.push(ArtifactId::Lean(*lean));
    }
    for policy in PolicyId::ALL {
        ids.push(ArtifactId::FinalPolicy(*policy));
    }
    for ir in IrId::ALL {
        ids.push(ArtifactId::Ir(*ir));
    }
    ids.into_iter().map(ArtifactId::spec).collect()
}

/// The number of committed fixtures each artifact family must have.
pub const FIXTURE_COUNTS: [(ArtifactKind, usize); 5] = [
    (ArtifactKind::Chart, 156),
    (ArtifactKind::Template, 20),
    (ArtifactKind::Lean, 4),
    (ArtifactKind::FinalPolicy, 4),
    (ArtifactKind::Ir, 18),
];

/// Checks the registry's shape before anything is produced.
///
/// Identities, keys and file names are unique; every fixture family has its
/// expected count; file names are single path components; fixture targets are
/// normal relative paths.
///
/// # Errors
///
/// Returns an error describing every violated rule.
pub fn validate(specs: &[ArtifactSpec]) -> eyre::Result<()> {
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for spec in specs {
        if !ids.insert(spec.id) {
            problems.push(format!("duplicate id {:?}", spec.id));
        }
        if !keys.insert(spec.id.key()) {
            problems.push(format!("duplicate key {}", spec.id.key()));
        }
        if !names.insert(spec.dump_name.as_str()) {
            problems.push(format!("duplicate file name {}", spec.dump_name));
        }
        if !is_safe_file_name(&spec.dump_name) {
            problems.push(format!("unsafe file name {:?}", spec.dump_name));
        }
        if let ArtifactTarget::Fixture(target) = &spec.target {
            if !is_safe_relative_path(target) {
                problems.push(format!("unsafe fixture path {target:?}"));
            }
            if !targets.insert(target.as_str()) {
                problems.push(format!("duplicate fixture path {target}"));
            }
        }
    }
    for (kind, expected) in FIXTURE_COUNTS {
        let count = specs
            .iter()
            .filter(|spec| {
                spec.id.kind() == kind && matches!(spec.target, ArtifactTarget::Fixture(_))
            })
            .count();
        if count != expected {
            problems.push(format!(
                "{} fixtures: {count}, expected {expected}",
                kind.name()
            ));
        }
    }
    eyre::ensure!(
        problems.is_empty(),
        "invalid artifact registry:\n{}",
        problems.join("\n")
    );
    Ok(())
}

/// Whether `name` is one plain file name: not empty, no separators, not `.`/`..`.
#[must_use]
pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

/// Whether `path` is relative and made only of plain `/`-separated file names.
#[must_use]
pub fn is_safe_relative_path(path: &str) -> bool {
    path.split('/').all(is_safe_file_name)
}

impl GenerationRecipe {
    /// A complete JSON description of this recipe for the manifest.
    ///
    /// Two recipes describe identically only when every field that feeds
    /// generation is identical.
    #[must_use]
    pub fn describe(&self) -> Value {
        match self {
            GenerationRecipe::Chart(chart) => json!({ "chart": describe_chart(chart) }),
            GenerationRecipe::Template(template) => json!({
                "template": {
                    "template_path": template.template_path,
                    "values_path": template.values_path,
                    "inline_values": template.inline_values,
                    "define_sources": describe_define_sources(template.define_sources),
                    "provider": match template.provider {
                        TemplateProvider::K8s(version) => json!({ "k8s": version }),
                        TemplateProvider::CrdK8s(version) => json!({ "crd+k8s": version }),
                    },
                    "provider_bundle": PROVIDER_BUNDLE,
                },
            }),
            GenerationRecipe::FinalPolicy(policy) => json!({
                "final_policy": {
                    "chart": describe_chart(&policy.chart),
                    "reference_policy": format!("{:?}", policy.reference_policy),
                    "override_file": policy.override_file.map(|file| json!({
                        "name": file.name,
                        "contents": file.contents,
                    })),
                    "strip_descriptions": false,
                    "minimize": true,
                },
            }),
            GenerationRecipe::Ir(ir) => json!({
                "ir": {
                    "template_path": ir.template_path,
                    "define_sources": describe_define_sources(ir.define_sources),
                },
            }),
        }
    }

    /// The chart directory this recipe reads, relative to `testdata/charts`.
    #[must_use]
    pub fn chart(&self) -> Option<&'static str> {
        match self {
            GenerationRecipe::Chart(chart) => Some(chart.chart),
            GenerationRecipe::FinalPolicy(policy) => Some(policy.chart.chart),
            GenerationRecipe::Template(_) | GenerationRecipe::Ir(_) => None,
        }
    }
}

/// The vendored provider bundle directory, relative to `testdata`.
pub const PROVIDER_BUNDLE: &str = "provider-bundle";

fn describe_chart(chart: &ChartRecipe) -> Value {
    json!({
        "chart": chart.chart,
        "profile": format!("{:?}", chart.profile),
        "include_tests": chart.include_tests,
        "include_subchart_values": chart.include_subchart_values,
        "values_files": chart.values_files,
        "infer_required": chart.infer_required,
        "k8s_versions": [chart.k8s_version],
        "provider_bundle": PROVIDER_BUNDLE,
        "allow_net": false,
        "minimize": chart.minimize,
    })
}

fn describe_define_sources(sources: test_util::DefineSourceSpec<'_>) -> Value {
    json!({
        "helper_templates": sources.helper_templates,
        "helper_template_dirs": sources.helper_template_dirs,
        "file_sources": sources.file_sources,
    })
}
