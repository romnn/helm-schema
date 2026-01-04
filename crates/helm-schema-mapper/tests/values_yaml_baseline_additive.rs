use color_eyre::eyre;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use test_util::prelude::*;
use vfs::VfsPath;

#[test]
fn values_yaml_is_additive_and_does_not_override_template_inference() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_root = root.join("crates/helm-schema-mapper/tests/fixtures/full-fixture")?;
    if !chart_root.exists()? {
        return Ok(());
    }

    let chart = load_chart(&chart_root, &LoadOptions::default())?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    // In full-fixture, auth.enabled is referenced in templates and should remain a boolean.
    let auth_enabled = schema
        .pointer("/properties/auth/properties/enabled/type")
        .ok_or_else(|| eyre::eyre!("missing auth.enabled.type"))?;
    assert_eq!(auth_enabled.as_str(), Some("boolean"));

    // Ingress annotations are commonly maps; ensure we at least produce an object schema.
    let annotations_type = schema
        .pointer("/properties/ingress/properties/annotations/type")
        .ok_or_else(|| eyre::eyre!("missing ingress.annotations.type"))?;
    assert_eq!(annotations_type.as_str(), Some("object"));

    Ok(())
}

#[test]
fn signoz_values_yaml_only_keys_are_present() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_root = root.join("crates/helm-schema-mapper/testdata/charts/signoz-signoz")?;
    if !chart_root.exists()? {
        // Optional fixture.
        return Ok(());
    }

    let chart = load_chart(&chart_root, &LoadOptions::default())?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    // clusterName exists in values.yaml but is not necessarily referenced in templates.
    let cluster_name_ty = schema
        .pointer("/properties/clusterName/type")
        .ok_or_else(|| eyre::eyre!("missing clusterName.type"))?;
    assert_eq!(cluster_name_ty.as_str(), Some("string"));

    // Step 2: dependency/subchart values.yaml should be composed under the subchart name.
    let clickhouse_enabled = schema
        .pointer("/properties/clickhouse/properties/zookeeper/properties/enabled/type")
        .ok_or_else(|| eyre::eyre!("missing clickhouse.zookeeper.enabled.type"))?;
    assert_eq!(clickhouse_enabled.as_str(), Some("boolean"));

    let clickhouse_registry = schema
        .pointer(
            "/properties/clickhouse/properties/global/properties/imageRegistry/type",
        )
        .ok_or_else(|| eyre::eyre!("missing clickhouse.global.imageRegistry.type"))?;
    assert_eq!(clickhouse_registry.as_str(), Some("null"));

    let gw_replica_count = schema
        .pointer("/properties/signoz-otel-gateway/properties/replicaCount/type")
        .ok_or_else(|| eyre::eyre!("missing signoz-otel-gateway.replicaCount.type"))?;
    assert_eq!(gw_replica_count.as_str(), Some("integer"));

    let gw_image_repo = schema
        .pointer(
            "/properties/signoz-otel-gateway/properties/image/properties/repository/type",
        )
        .ok_or_else(|| eyre::eyre!("missing signoz-otel-gateway.image.repository.type"))?;
    assert_eq!(gw_image_repo.as_str(), Some("string"));

    Ok(())
}
