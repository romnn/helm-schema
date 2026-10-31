//! Whole-chart navigation carriers and independently executed host requirements.

use color_eyre::eyre;
use helm_schema::generation::DeclaredTypes;
use helm_schema_test_support::generate::chart_schema;
use helm_schema_test_support::registry::{ArtifactId, ChartId, GenerationRecipe};
use serde_json::json;
use test_util::helm_values::coalesce_chart_values;
use test_util::prelude::sim_assert_eq;

#[test]
fn harbor_navigation_hosts_under_assert() -> eyre::Result<()> {
    harbor_navigation_hosts(DeclaredTypes::Assert)
}

#[test]
fn harbor_navigation_hosts_under_annotate() -> eyre::Result<()> {
    harbor_navigation_hosts(DeclaredTypes::Annotate)
}

fn harbor_navigation_hosts(declared_types: DeclaredTypes) -> eyre::Result<()> {
    let schema = harbor_schema(declared_types)?;
    let validator = jsonschema::draft7::new(&schema)?;
    let chart = test_util::workspace_testdata().join("charts/harbor");
    let values = coalesce_chart_values(&chart, json!({ "internalTLS": { "core": true } }))?;
    sim_assert_eq!(have: validator.is_valid(&values), want: true);
    Ok(())
}

#[test]
fn harbor_live_tls_rejects_a_scalar_host() -> eyre::Result<()> {
    let chart = test_util::workspace_testdata().join("charts/harbor");
    for declared_types in [DeclaredTypes::Assert, DeclaredTypes::Annotate] {
        let schema = harbor_schema(declared_types)?;
        let validator = jsonschema::draft7::new(&schema)?;
        for cert_source in ["manual", "secret"] {
            // Supply sibling material so missing certificates cannot mask the invalid host.
            let overlay = json!({ "internalTLS": {
                "enabled": true, "certSource": cert_source, "core": true,
                "trustCa": "ca",
                "portal": { "crt": "crt", "key": "key", "secretName": "portal-tls" },
                "jobservice": { "crt": "crt", "key": "key", "secretName": "jobservice-tls" },
                "registry": { "crt": "crt", "key": "key", "secretName": "registry-tls" },
                "trivy": { "crt": "crt", "key": "key", "secretName": "trivy-tls" },
            } });
            let values = coalesce_chart_values(&chart, overlay.clone())?;
            sim_assert_eq!(have: validator.is_valid(&values), want: false, "{declared_types:?}: {overlay}");
        }
    }
    Ok(())
}

fn harbor_schema(declared_types: DeclaredTypes) -> eyre::Result<serde_json::Value> {
    let GenerationRecipe::Chart(mut recipe) = ArtifactId::Chart(ChartId::Harbor).spec().recipe
    else {
        eyre::bail!("Harbor must use a whole-chart recipe");
    };
    recipe.authoring.declared_types = declared_types;
    chart_schema(&recipe)
}
