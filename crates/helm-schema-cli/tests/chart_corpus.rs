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
//! The chart list and each chart's generation recipe live in the
//! `helm-schema-test-support` registry. Each test consumes its chart's
//! artifact: from the producer run named by `HELM_SCHEMA_CORPUS_ARTIFACTS`
//! after verifying the manifest against the checkout, or, without one, by
//! generating that chart locally. To regenerate fixtures after an intentional
//! generator change, run `cargo run -p helm-schema-test-support --bin
//! corpus_generation -- --out <dir>`, review the chart artifacts
//! (`helm-schema.cli.chart-corpus.<chart>.schema.json`), and copy the
//! adjudicated ones into `testdata/chart-corpus-schemas/`.
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
#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/values_validation.rs"]
mod values_validation;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema_test_support::registry::{ArtifactId, ArtifactTarget, ChartId};
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
    "eck-stack",
    "gitea",
    "headscale",
    "imgproxy",
    "kube-starrocks",
    "milvus",
    "nacos",
    "netbox",
    "nginx-ingress",
    "openebs",
    "openldap-stack-ha",
    "stacks-blockchain-api",
    "synapse",
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

/// Charts Helm cannot load at all, so it validates no values document for
/// them; the coalescer must report exactly the registered reason. Every other
/// chart must compose, and any other refusal fails the test.
const HELM_UNLOADABLE_CHARTS: &[(&str, &str)] = &[("cert-manager", "Chart.yaml file is missing")];

/// The chart's coalesced defaults against its schema: rejected where the
/// corpus records a rejection, accepted everywhere else.
fn assert_values_document(chart: &str, values_json: &Value, schema: &Value) {
    if KNOWN_VALUES_REJECTIONS.contains(&chart) {
        let errors = values_validation::validate_json_against_schema(values_json, schema);
        assert!(
            !errors.is_empty(),
            "{chart}: values.yaml now validates; remove it from KNOWN_VALUES_REJECTIONS"
        );
    } else if QUARANTINED_FALSE_REJECTIONS.contains(&chart) {
        let errors = values_validation::validate_json_against_schema(values_json, schema);
        assert!(
            !errors.is_empty(),
            "{chart}: the false rejection is fixed. Adjudicate the new fixture and \
             remove it from QUARANTINED_FALSE_REJECTIONS"
        );
    } else {
        values_validation::assert_values_json_validates(values_json, schema);
    }
}

fn assert_chart_schema_fixture(chart_id: ChartId) -> eyre::Result<()> {
    let chart = chart_id.relative_path();
    let spec = ArtifactId::Chart(chart_id).spec();
    let ArtifactTarget::Fixture(fixture) = &spec.target else {
        eyre::bail!("{chart}: registered without a chart-corpus fixture");
    };
    let schema = helm_schema_test_support::consume(spec.id)?;

    let unloadable = HELM_UNLOADABLE_CHARTS
        .iter()
        .find(|(unloadable, _)| *unloadable == chart)
        .map(|(_, reason)| *reason);
    let values_json = match values_validation::values_yaml_as_json_for_path(chart) {
        Ok(_) if unloadable.is_some() => {
            eyre::bail!("{chart}: Helm loads this chart now; remove it from HELM_UNLOADABLE_CHARTS")
        }
        Ok(values_json) => Some(values_json),
        Err(error) => match (
            unloadable,
            error.downcast_ref::<test_util::helm_values::ValuesError>(),
        ) {
            (Some(reason), Some(test_util::helm_values::ValuesError::NotValidated(actual))) => {
                assert!(
                    actual.contains(reason),
                    "{chart}: Helm refuses the chart for another reason than registered: {actual}"
                );
                None
            }
            _ => return Err(error),
        },
    };
    if let Some(values_json) = &values_json {
        assert_values_document(chart, values_json, &schema);
    }

    let fixture_path = test_util::workspace_root().join(fixture);
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(&fixture_path)
            .wrap_err_with(|| format!("read fixture {}", fixture_path.display()))?,
    )
    .wrap_err("parse fixture JSON")?;
    sim_assert_eq!(have: schema, want: expected, "{chart}: schema fixture mismatch");
    Ok(())
}

macro_rules! chart_schema_case {
    ($(($name:ident, $variant:ident, $chart:literal)),* $(,)?) => {
        $(
            #[test]
            fn $name() -> eyre::Result<()> {
                assert_chart_schema_fixture(ChartId::$variant)
            }
        )*
    };
}

helm_schema_test_support::corpus_charts!(chart_schema_case);

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
