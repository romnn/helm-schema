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
//! generator defect recorded in `plan/chart-corpus-expansion.md`. Their tests
//! pin the defect: once generation improves, the test fails and the entry
//! must be removed alongside a fixture update.

use test_util::prelude::sim_assert_eq;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;
#[path = "common/values_validation.rs"]
mod values_validation;

use color_eyre::eyre::{OptionExt as _, WrapErr as _};
use serde_json::Value;

/// Charts whose shipped `values.yaml` currently fails validation against the
/// schema we generate for them. See `plan/chart-corpus-expansion.md` for the
/// per-chart defect analysis.
const KNOWN_VALUES_REJECTIONS: &[&str] = &[
    "cilium",
    "grafana",
    "kube-prometheus-stack",
    "kyverno",
    "loki",
];

/// Charts whose full schema is not pinned as a fixture. The only entry is
/// kube-prometheus-stack: its generated schema is currently ~20 MB compact
/// because whole-CRD typed subtrees (PrometheusSpec and friends) are inlined
/// per conditional overlay arm instead of shared through `$defs`. That size
/// pathology is a round-2 finding in `plan/chart-corpus-expansion.md`; until
/// it is fixed, the chart pins its top-level key set instead.
const UNPINNED_SCHEMAS: &[&str] = &["kube-prometheus-stack"];

const KUBE_PROMETHEUS_STACK_TOP_LEVEL_KEYS: &[&str] = &[
    "additionalPrometheusRules",
    "additionalPrometheusRulesMap",
    "alertmanager",
    "cleanPrometheusOperatorObjectNames",
    "commonLabels",
    "coreDns",
    "crds",
    "customRules",
    "defaultRules",
    "extraManifests",
    "fullnameOverride",
    "global",
    "grafana",
    "kube-state-metrics",
    "kubeApiServer",
    "kubeControllerManager",
    "kubeDns",
    "kubeEtcd",
    "kubeProxy",
    "kubeScheduler",
    "kubeStateMetrics",
    "kubeTargetVersionOverride",
    "kubeVersionOverride",
    "kubelet",
    "kubernetesServiceMonitors",
    "nameOverride",
    "namespaceOverride",
    "nodeExporter",
    "prometheus",
    "prometheus-node-exporter",
    "prometheus-windows-exporter",
    "prometheusOperator",
    "thanosRuler",
    "windowsMonitoring",
];

fn assert_chart_schema_fixture(chart: &str) -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path(chart)?;

    if std::env::var("SCHEMA_DUMP").is_ok() {
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
            "{chart}: values.yaml now validates; remove it from KNOWN_VALUES_REJECTIONS \
             and close the corresponding finding in plan/chart-corpus-expansion.md"
        );
    } else {
        values_validation::assert_values_json_validates(&values_json, &schema);
    }

    if UNPINNED_SCHEMAS.contains(&chart) {
        let mut top_level_keys: Vec<&str> = schema
            .get("properties")
            .and_then(Value::as_object)
            .ok_or_eyre("schema.properties must be an object")?
            .keys()
            .map(String::as_str)
            .collect();
        top_level_keys.sort_unstable();
        sim_assert_eq!(
            have: top_level_keys,
            want: KUBE_PROMETHEUS_STACK_TOP_LEVEL_KEYS.to_vec(),
            "{chart}: top-level schema keys changed"
        );
        return Ok(());
    }

    let fixture_path = test_util::workspace_testdata()
        .join("chart-corpus-schemas")
        .join(format!("{chart}.schema.json"));
    if !fixture_path.exists() && std::env::var("SCHEMA_DUMP").is_ok() {
        // Bootstrap mode: the dump above is the fixture candidate.
        return Ok(());
    }
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
        fn $name() -> color_eyre::eyre::Result<()> {
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
