//! Whole-chart schema fixtures for every vendored chart in `testdata/charts`.
//!
//! Each case runs the production generation pipeline over a vendored chart
//! (offline, workspace-local schema caches, subchart values included), pins
//! the full generated schema as a fixture under
//! `testdata/chart-corpus-schemas/`, and checks the chart's own `values.yaml`
//! against the generated schema. Chart-specific SEMANTIC assertions (helm
//! sample validation, description placement, guard accept/reject behavior)
//! stay in their own test files (`chart_signoz_signoz.rs`,
//! `chart_bitnami_redis.rs`, `chart_signoz_postgresql.rs`): fixture equality
//! pins what the output currently is, behavior tests pin what must be true,
//! and only the latter protect fixture regeneration from pinning a
//! regression.
//!
//! To regenerate fixtures after an intentional generator change, run
//! `SCHEMA_DUMP=1 cargo nextest run -p helm-schema-cli --no-fail-fast -E
//! 'binary(chart_corpus)'`, review the dumps written to the system temp
//! directory (`helm-schema.cli.chart-corpus.<chart>.schema.json`), and copy
//! the adjudicated ones into `testdata/chart-corpus-schemas/`.
//!
//! Charts whose own `values.yaml` is currently rejected by the generated
//! schema are listed in `KNOWN_VALUES_REJECTIONS`; each entry is a known
//! generator defect. Their tests pin the defect: once generation improves,
//! the test fails and the entry must be removed alongside a fixture
//! update.
//!
//! Charts vendored by the corpus-expansion-v1 intake are listed in
//! `UNADJUDICATED_INTAKE`, and the subset already proven wrong is listed in
//! `QUARANTINED_FALSE_REJECTIONS`.
//! Their fixtures pin **current** output rather than correct output, so read
//! those doc comments before treating a diff against one as a regression.

use test_util::prelude::sim_assert_eq;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;
#[path = "common/values_validation.rs"]
mod values_validation;
#[path = "common/values_yaml.rs"]
mod values_yaml;

use color_eyre::eyre::{self, WrapErr as _};
use serde_json::Value;

/// Charts whose shipped `values.yaml` fails validation against the schema
/// we generate for them. Keep this empty unless the CHART's own defaults
/// genuinely fail `helm template`: a new entry otherwise means a new
/// generator defect, and it should only be added together with a written
/// analysis.
///
/// - aws-load-balancer-controller: `clusterName` defaults to `""` and the
///   deployment calls `required` on it; `helm template` with defaults
///   fails with "Chart cannot be installed without a valid clusterName!".
/// - karpenter: `settings.clusterName` defaults to `""` with the same
///   `required` termination in its deployment.
/// - schema-emission-unconditional-fail: the profile control deliberately
///   executes an unguarded `fail`; full must reject every values document.
// loki: `helm template` genuinely fails on pure defaults ("Please define
// loki.storage.bucketNames.chunks"): `loki.schemaConfig` defaults to `{}`
// (falsy) and `useTestSchema` to false, so validate.yaml's schema-config
// `fail` plus the bucketNames `required` chain reject the shipped values.
const KNOWN_VALUES_REJECTIONS: &[&str] = &[
    "aws-load-balancer-controller",
    "karpenter",
    "loki",
    "schema-emission-unconditional-fail",
];

/// Charts whose shipped `values.yaml` is rejected by a schema that should have
/// accepted it, because the generator is wrong rather than the chart.
///
/// `helm template` renders each of these charts on its own defaults, so every
/// entry is a confirmed false rejection.
/// Each was surveyed and adjudicated in `plan/corpus-expansion-v1.md`.
///
/// This is the inverse of [`KNOWN_VALUES_REJECTIONS`], where the chart itself
/// refuses its defaults and agreeing with it is the correct outcome.
/// Listing the defects keeps them pinned and visible instead of absent from the
/// corpus entirely.
/// A landing fix therefore breaks the corresponding test on purpose, and the
/// chart must be promoted out of this list by the same change that adjudicates
/// its new fixture.
const QUARANTINED_FALSE_REJECTIONS: &[&str] = &[
    "aws-ebs-csi-driver",
    "dify",
    "eck-stack",
    "gitea",
    "graylog",
    "headscale",
    "imgproxy",
    "kube-starrocks",
    "kubeshark",
    "milvus",
    "nacos",
    "netbox",
    "nginx-ingress",
    "okteto",
    "oncall",
    "openebs",
    "openldap-stack-ha",
    "redmine",
    "schema-registry",
    "spinnaker",
    "stacks-blockchain-api",
    "synapse",
    "weblate",
    "yourls",
];

/// Charts vendored in one batch from the Artifact Hub popularity ranking, whose
/// fixtures are pinned for **drift detection only**.
///
/// None of these schemas has been adjudicated against real `helm template`
/// behavior, unlike the charts that predate this intake.
/// A fixture here therefore records what the generator currently emits rather
/// than what is correct, so a diff against one means "output changed" and never
/// "output regressed".
/// Adjudicating them is the separate sweep this intake exists to enable, and
/// `plan/corpus-expansion-v1.md` carries its findings.
///
/// The subset already proven wrong is additionally listed in
/// [`QUARANTINED_FALSE_REJECTIONS`].
/// Promote a chart out of this roster once its schema has been adjudicated.
const UNADJUDICATED_INTAKE: &[&str] = &[
    "aws-ebs-csi-driver",
    "dify",
    "eck-stack",
    "gitea",
    "graylog",
    "headscale",
    "imgproxy",
    "kube-starrocks",
    "kubeshark",
    "milvus",
    "nacos",
    "netbox",
    "nginx-ingress",
    "okteto",
    "oncall",
    "openebs",
    "openldap-stack-ha",
    "redmine",
    "schema-registry",
    "spinnaker",
    "stacks-blockchain-api",
    "synapse",
    "weblate",
    "yourls",
    "actions-runner-controller",
    "alertmanager",
    "alloy",
    "apisix",
    "argo-events",
    "argo-rollouts",
    "argo-workflows",
    "argocd-apps",
    "argocd-image-updater",
    "aws-for-fluent-bit",
    "aws-node-termination-handler",
    "base",
    "chartmuseum",
    "clickhouse",
    "consul",
    "descheduler",
    "dex",
    "eck-operator",
    "elasticsearch",
    "etcd",
    "filebeat",
    "fluentd",
    "gateway",
    "gitlab-runner",
    "goldilocks",
    "headlamp",
    "home-assistant",
    "influxdb",
    "jira",
    "jupyterhub",
    "kafka-ui",
    "keycloakx",
    "kibana",
    "kubernetes-event-exporter",
    "kubeview",
    "kured",
    "logstash",
    "mailhog",
    "mariadb",
    "mariadb-galera",
    "metabase",
    "minecraft",
    "nfs-server-provisioner",
    "nginx",
    "nginx-ingress-controller",
    "ollama",
    "open-webui",
    "opensearch",
    "opentelemetry-operator",
    "pgadmin4",
    "phpmyadmin",
    "pihole",
    "postgresql-ha",
    "prometheus-adapter",
    "prometheus-blackbox-exporter",
    "prometheus-node-exporter",
    "prometheus-operator-crds",
    "prometheus-pushgateway",
    "prometheus-redis-exporter",
    "qdrant",
    "rabbitmq",
    "rabbitmq-cluster-operator",
    "rancher",
    "redis-cluster",
    "redis-ha",
    "rook-ceph",
    "spark",
    "terraform",
    "tigera-operator",
    "trino",
    "uptime-kuma",
    "vector",
    "vpa",
    "x509-certificate-exporter",
    "zabbix",
    "zookeeper",
];

fn assert_chart_schema_fixture(chart: &str) -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path(chart)?;
    let dump = std::env::var("SCHEMA_DUMP").is_ok();

    if dump {
        let path =
            std::env::temp_dir().join(format!("helm-schema.cli.chart-corpus.{chart}.schema.json"));
        let mut bytes =
            serde_json::to_vec_pretty(&schema).wrap_err("serialize chart corpus schema dump")?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes).wrap_err("write chart corpus schema dump")?;
    }

    let values_json = values_validation::values_yaml_as_json_for_path(chart)?;
    if KNOWN_VALUES_REJECTIONS.contains(&chart) {
        let errors = values_validation::validate_json_against_schema(&values_json, &schema);
        assert!(
            !errors.is_empty(),
            "{chart}: values.yaml now validates; remove it from KNOWN_VALUES_REJECTIONS"
        );
    } else if QUARANTINED_FALSE_REJECTIONS.contains(&chart) {
        let errors = values_validation::validate_json_against_schema(&values_json, &schema);
        assert!(
            !errors.is_empty(),
            "{chart}: the false rejection is fixed. Adjudicate the new fixture and \
             remove it from QUARANTINED_FALSE_REJECTIONS"
        );
    } else {
        values_validation::assert_values_json_validates(&values_json, &schema);
    }
    if dump {
        return Ok(());
    }

    let fixture_path = test_util::workspace_testdata()
        .join("chart-corpus-schemas")
        .join(format!("{chart}.schema.json"));
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(&fixture_path)
            .wrap_err_with(|| format!("read fixture {}", fixture_path.display()))?,
    )
    .wrap_err("parse fixture JSON")?;
    sim_assert_eq!(have: schema, want: expected, "{chart}: schema fixture mismatch");
    Ok(())
}

macro_rules! chart_schema_case {
    ($name:ident, $chart:literal) => {
        #[test]
        fn $name() -> eyre::Result<()> {
            assert_chart_schema_fixture($chart)
        }
    };
}

chart_schema_case!(airflow, "airflow");
chart_schema_case!(argo_cd, "argo-cd");
chart_schema_case!(aws_load_balancer_controller, "aws-load-balancer-controller");
chart_schema_case!(bitnami_postgresql, "bitnami-postgresql");
chart_schema_case!(bitnami_redis, "bitnami-redis");
chart_schema_case!(cert_manager, "cert-manager");
chart_schema_case!(cilium, "cilium");
chart_schema_case!(cloudnative_pg, "cloudnative-pg");
chart_schema_case!(cluster_autoscaler, "cluster-autoscaler");
chart_schema_case!(common, "common");
chart_schema_case!(coredns, "coredns");
chart_schema_case!(crossplane, "crossplane");
chart_schema_case!(datadog, "datadog");
chart_schema_case!(dict_config, "dict-config");
chart_schema_case!(external_dns, "external-dns");
chart_schema_case!(external_secrets, "external-secrets");
chart_schema_case!(falco, "falco");
chart_schema_case!(fluent_bit, "fluent-bit");
chart_schema_case!(flux2, "flux2");
chart_schema_case!(grafana, "grafana");
chart_schema_case!(harbor, "harbor");
chart_schema_case!(ingress_nginx, "ingress-nginx");
chart_schema_case!(istiod, "istiod");
chart_schema_case!(jaeger, "jaeger");
chart_schema_case!(jenkins, "jenkins");
chart_schema_case!(karpenter, "karpenter");
chart_schema_case!(keda, "keda");
chart_schema_case!(kube_prometheus_stack, "kube-prometheus-stack");
chart_schema_case!(kube_state_metrics, "kube-state-metrics");
chart_schema_case!(kyverno, "kyverno");
chart_schema_case!(loki, "loki");
chart_schema_case!(longhorn, "longhorn");
chart_schema_case!(metallb, "metallb");
chart_schema_case!(metrics_server, "metrics-server");
chart_schema_case!(minio, "minio");
chart_schema_case!(nack, "nack");
chart_schema_case!(nats, "nats");
chart_schema_case!(nats_account_server, "nats-account-server");
chart_schema_case!(nats_kafka, "nats-kafka");
chart_schema_case!(nats_operator, "nats-operator");
chart_schema_case!(
    nfs_subdir_external_provisioner,
    "nfs-subdir-external-provisioner"
);
chart_schema_case!(oauth2_proxy, "oauth2-proxy");
chart_schema_case!(prometheus, "prometheus");
chart_schema_case!(promtail, "promtail");
chart_schema_case!(reloader, "reloader");
chart_schema_case!(
    schema_emission_unconditional_fail,
    "schema-emission-unconditional-fail"
);
chart_schema_case!(sealed_secrets, "sealed-secrets");
chart_schema_case!(signoz_signoz, "signoz-signoz");
chart_schema_case!(surveyor, "surveyor");
chart_schema_case!(tempo, "tempo");
chart_schema_case!(traefik, "traefik");
chart_schema_case!(trivy_operator, "trivy-operator");
chart_schema_case!(vault, "vault");
chart_schema_case!(velero, "velero");
chart_schema_case!(zalando_postgres_operator, "zalando-postgres-operator");
chart_schema_case!(zalando_postgres_operator_ui, "zalando-postgres-operator-ui");

// Corpus-expansion-v1 intake, unadjudicated
chart_schema_case!(aws_ebs_csi_driver, "aws-ebs-csi-driver");
chart_schema_case!(dify, "dify");
chart_schema_case!(eck_stack, "eck-stack");
chart_schema_case!(gitea, "gitea");
chart_schema_case!(graylog, "graylog");
chart_schema_case!(headscale, "headscale");
chart_schema_case!(imgproxy, "imgproxy");
chart_schema_case!(kube_starrocks, "kube-starrocks");
chart_schema_case!(kubeshark, "kubeshark");
chart_schema_case!(milvus, "milvus");
chart_schema_case!(nacos, "nacos");
chart_schema_case!(netbox, "netbox");
chart_schema_case!(nginx_ingress, "nginx-ingress");
chart_schema_case!(okteto, "okteto");
chart_schema_case!(oncall, "oncall");
chart_schema_case!(openebs, "openebs");
chart_schema_case!(openldap_stack_ha, "openldap-stack-ha");
chart_schema_case!(redmine, "redmine");
chart_schema_case!(schema_registry, "schema-registry");
chart_schema_case!(spinnaker, "spinnaker");
chart_schema_case!(stacks_blockchain_api, "stacks-blockchain-api");
chart_schema_case!(synapse, "synapse");
chart_schema_case!(weblate, "weblate");
chart_schema_case!(yourls, "yourls");
chart_schema_case!(actions_runner_controller, "actions-runner-controller");
chart_schema_case!(alertmanager, "alertmanager");
chart_schema_case!(alloy, "alloy");
chart_schema_case!(apisix, "apisix");
chart_schema_case!(argo_events, "argo-events");
chart_schema_case!(argo_rollouts, "argo-rollouts");
chart_schema_case!(argo_workflows, "argo-workflows");
chart_schema_case!(argocd_apps, "argocd-apps");
chart_schema_case!(argocd_image_updater, "argocd-image-updater");
chart_schema_case!(aws_for_fluent_bit, "aws-for-fluent-bit");
chart_schema_case!(aws_node_termination_handler, "aws-node-termination-handler");
chart_schema_case!(base, "base");
chart_schema_case!(chartmuseum, "chartmuseum");
chart_schema_case!(clickhouse, "clickhouse");
chart_schema_case!(consul, "consul");
chart_schema_case!(descheduler, "descheduler");
chart_schema_case!(dex, "dex");
chart_schema_case!(eck_operator, "eck-operator");
chart_schema_case!(elasticsearch, "elasticsearch");
chart_schema_case!(etcd, "etcd");
chart_schema_case!(filebeat, "filebeat");
chart_schema_case!(fluentd, "fluentd");
chart_schema_case!(gateway, "gateway");
chart_schema_case!(gitlab_runner, "gitlab-runner");
chart_schema_case!(goldilocks, "goldilocks");
chart_schema_case!(headlamp, "headlamp");
chart_schema_case!(home_assistant, "home-assistant");
chart_schema_case!(influxdb, "influxdb");
chart_schema_case!(jira, "jira");
chart_schema_case!(jupyterhub, "jupyterhub");
chart_schema_case!(kafka_ui, "kafka-ui");
chart_schema_case!(keycloakx, "keycloakx");
chart_schema_case!(kibana, "kibana");
chart_schema_case!(kubernetes_event_exporter, "kubernetes-event-exporter");
chart_schema_case!(kubeview, "kubeview");
chart_schema_case!(kured, "kured");
chart_schema_case!(logstash, "logstash");
chart_schema_case!(mailhog, "mailhog");
chart_schema_case!(mariadb, "mariadb");
chart_schema_case!(mariadb_galera, "mariadb-galera");
chart_schema_case!(metabase, "metabase");
chart_schema_case!(minecraft, "minecraft");
chart_schema_case!(nfs_server_provisioner, "nfs-server-provisioner");
chart_schema_case!(nginx, "nginx");
chart_schema_case!(nginx_ingress_controller, "nginx-ingress-controller");
chart_schema_case!(ollama, "ollama");
chart_schema_case!(open_webui, "open-webui");
chart_schema_case!(opensearch, "opensearch");
chart_schema_case!(opentelemetry_operator, "opentelemetry-operator");
chart_schema_case!(pgadmin4, "pgadmin4");
chart_schema_case!(phpmyadmin, "phpmyadmin");
chart_schema_case!(pihole, "pihole");
chart_schema_case!(postgresql_ha, "postgresql-ha");
chart_schema_case!(prometheus_adapter, "prometheus-adapter");
chart_schema_case!(prometheus_blackbox_exporter, "prometheus-blackbox-exporter");
chart_schema_case!(prometheus_node_exporter, "prometheus-node-exporter");
chart_schema_case!(prometheus_operator_crds, "prometheus-operator-crds");
chart_schema_case!(prometheus_pushgateway, "prometheus-pushgateway");
chart_schema_case!(prometheus_redis_exporter, "prometheus-redis-exporter");
chart_schema_case!(qdrant, "qdrant");
chart_schema_case!(rabbitmq, "rabbitmq");
chart_schema_case!(rabbitmq_cluster_operator, "rabbitmq-cluster-operator");
chart_schema_case!(rancher, "rancher");
chart_schema_case!(redis_cluster, "redis-cluster");
chart_schema_case!(redis_ha, "redis-ha");
chart_schema_case!(rook_ceph, "rook-ceph");
chart_schema_case!(spark, "spark");
chart_schema_case!(terraform, "terraform");
chart_schema_case!(tigera_operator, "tigera-operator");
chart_schema_case!(trino, "trino");
chart_schema_case!(uptime_kuma, "uptime-kuma");
chart_schema_case!(vector, "vector");
chart_schema_case!(vpa, "vpa");
chart_schema_case!(x509_certificate_exporter, "x509-certificate-exporter");
chart_schema_case!(zabbix, "zabbix");
chart_schema_case!(zookeeper, "zookeeper");

/// Guards the two quarantine rosters against drifting apart as charts are
/// promoted out of them.
///
/// The rosters are hand-maintained and read by humans deciding whether a fixture
/// diff is a regression, so an inconsistency between them silently misleads that
/// decision.
#[test]
fn quarantine_rosters_are_consistent() {
    for chart in QUARANTINED_FALSE_REJECTIONS {
        // A known-wrong chart that escaped the intake roster would present as
        // adjudicated, which is exactly the wrong signal.
        assert!(
            UNADJUDICATED_INTAKE.contains(chart),
            "{chart}: listed as a known false rejection but missing from UNADJUDICATED_INTAKE"
        );
        // The two rejection lists encode opposite verdicts about who is at
        // fault, so no chart can legitimately appear in both.
        assert!(
            !KNOWN_VALUES_REJECTIONS.contains(chart),
            "{chart}: cannot be both a chart-side rejection and a generator-side one"
        );
    }

    // A duplicated entry would survive a promotion that removes only one copy,
    // leaving the chart quarantined by an invisible second listing.
    let mut sorted = UNADJUDICATED_INTAKE.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sim_assert_eq!(have: sorted.len(), want: UNADJUDICATED_INTAKE.len());
}
