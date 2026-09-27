//! Full-schema and values-validation regressions for the `SigNoz PostgreSQL` chart.

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/helm_samples.rs"]
mod helm_samples;
#[path = "common/values_validation.rs"]
mod values_validation;

use color_eyre::eyre;
use helm_schema_test_support::{ArtifactId, ChartId};

use indoc::indoc;

#[test]
fn signoz_postgresql_values_yaml_and_guard_samples_validate() -> eyre::Result<()> {
    let chart_path = "signoz-signoz/charts/signoz-otel-gateway/charts/postgresql";
    let schema = helm_schema_test_support::consume(ArtifactId::Chart(ChartId::SignozPostgresql))?;
    let values_json = values_validation::values_yaml_as_json_for_path(chart_path)?;
    values_validation::assert_values_json_validates(&values_json, &schema);
    helm_samples::assert_generated_schema_accepts_helm_samples_for_path(
        chart_path,
        &schema,
        &[
            helm_samples::HelmValidationSample {
                name: "default",
                values_yaml: None,
            },
            helm_samples::HelmValidationSample {
                name: "replication-with-metrics",
                values_yaml: Some(indoc! {"
                    architecture: replication
                    auth:
                      database: app
                    metrics:
                      enabled: true
                    readReplicas:
                      replicaCount: 2
                "}),
            },
        ],
    )?;

    Ok(())
}
