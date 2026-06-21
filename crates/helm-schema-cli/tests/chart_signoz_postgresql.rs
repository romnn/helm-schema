#[path = "common/helm_samples.rs"]
mod helm_samples;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

use indoc::indoc;

#[test]
fn signoz_postgresql_values_yaml_and_guard_samples_validate() -> color_eyre::eyre::Result<()> {
    let chart_path = "signoz-signoz/charts/signoz-otel-gateway/charts/postgresql";
    let schema = schema_roundtrip::generate_chart_schema_for_path(chart_path)?;
    let values_json = schema_roundtrip::values_yaml_as_json_for_path(chart_path)?;
    schema_roundtrip::assert_values_json_validates(&values_json, &schema);
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
