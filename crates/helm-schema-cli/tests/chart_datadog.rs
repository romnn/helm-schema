//! Semantic assertions for the datadog chart. Values validation and the
//! full-schema pin live in `chart_corpus.rs`.

use color_eyre::eyre::{self, WrapErr as _};
use test_util::prelude::sim_assert_eq;

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;
#[path = "common/values_yaml.rs"]
mod values_yaml;

#[test]
fn datadog_image_tag_accepts_non_strings() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("datadog")?;
    let validator = jsonschema::validator_for(&schema)?;

    // Cases compose over the chart defaults: helm validates the coalesced
    // document, and the chart navigates hosts these overrides do not touch.
    for tag in [serde_json::json!(7), serde_json::json!("7.68.2")] {
        assert!(
            validator.is_valid(&chart_instances::with_override(
                "datadog",
                serde_json::json!({
                    "agents": { "image": { "tag": tag, "doNotCheckTag": true } }
                })
            )?),
            "toString converts the tag before trimSuffix consumes it: tag={tag}"
        );
    }
    Ok(())
}

#[test]
fn datadog_active_member_hosts_reject_scalars() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("datadog")?;
    let validator = jsonschema::validator_for(&schema)?;

    // Helm reaches each member read under the chart defaults. The operator
    // case fails earlier while Helm coalesces the aliased dependency.
    let outcomes = [
        (
            "clusterChecksRunner.rbac",
            serde_json::json!({ "clusterChecksRunner": { "rbac": 7 } }),
        ),
        (
            "datadog.discovery.networkStats",
            serde_json::json!({ "datadog": { "discovery": { "networkStats": 7 } } }),
        ),
        (
            "datadog.serviceMonitoring.tls",
            serde_json::json!({ "datadog": { "serviceMonitoring": { "tls": 7 } } }),
        ),
        ("operator", serde_json::json!({ "operator": 7 })),
    ]
    .into_iter()
    .map(|(path, override_value)| {
        let instance = chart_instances::with_override("datadog", override_value)?;
        Ok((path, validator.is_valid(&instance)))
    })
    .collect::<eyre::Result<Vec<_>>>()?;
    sim_assert_eq!(
        have: outcomes,
        want: vec![
            ("clusterChecksRunner.rbac", false),
            ("datadog.discovery.networkStats", false),
            ("datadog.serviceMonitoring.tls", false),
            ("operator", false),
        ],
        "every scalar host aborts in Helm"
    );
    Ok(())
}

#[test]
fn datadog_fips_root_is_required_by_the_live_helper_chain() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("datadog")?;
    let validator = jsonschema::validator_for(&schema)?;
    let instance = chart_instances::with_override("datadog", serde_json::json!({ "fips": null }))?;

    assert!(
        !validator.is_valid(&instance),
        "deleting fips reaches the helper-gated .Values.fips.enabled read"
    );
    Ok(())
}

#[test]
fn datadog_renderable_ci_values_remain_accepted() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("datadog")?;
    let validator = jsonschema::validator_for(&schema)?;
    let chart_dir = schema_roundtrip::physical_chart_dir("datadog");

    let outcomes = ["autoscaling-values.yaml", "gke-gdc-values.yaml"]
        .into_iter()
        .map(|name| {
            let path = chart_dir.join("ci").join(name);
            let source = std::fs::read_to_string(&path)
                .wrap_err_with(|| format!("read Datadog CI values {}", path.display()))?;
            let override_value = serde_yaml::from_str(&source)
                .wrap_err_with(|| format!("parse Datadog CI values {}", path.display()))?;
            let instance = chart_instances::with_override("datadog", override_value)?;
            Ok((name, validator.is_valid(&instance)))
        })
        .collect::<eyre::Result<Vec<_>>>()?;

    sim_assert_eq!(
        have: outcomes,
        want: vec![
            ("autoscaling-values.yaml", true),
            ("gke-gdc-values.yaml", true),
        ],
        "Helm renders both coalesced values documents"
    );
    Ok(())
}
