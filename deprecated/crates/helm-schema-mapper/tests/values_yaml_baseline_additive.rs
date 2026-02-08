use color_eyre::eyre;
use helm_schema_chart::{LoadOptions, load_chart};
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
        .pointer("/properties/clickhouse/properties/global/properties/imageRegistry/type")
        .ok_or_else(|| eyre::eyre!("missing clickhouse.global.imageRegistry.type"))?;
    assert_eq!(clickhouse_registry.as_str(), Some("null"));

    let gw_replica_count = schema
        .pointer("/properties/signoz-otel-gateway/properties/replicaCount/type")
        .ok_or_else(|| eyre::eyre!("missing signoz-otel-gateway.replicaCount.type"))?;
    assert_eq!(gw_replica_count.as_str(), Some("integer"));

    let gw_image_repo = schema
        .pointer("/properties/signoz-otel-gateway/properties/image/properties/repository/type")
        .ok_or_else(|| eyre::eyre!("missing signoz-otel-gateway.image.repository.type"))?;
    assert_eq!(gw_image_repo.as_str(), Some("string"));

    // Step 3: regression guard against common false positives.
    // Helm builtins like `.Release` should never become `.Values`-ish schema keys.
    let forbidden = [
        "/properties/otelCollector/properties/Release",
        "/properties/otelCollector/properties/Chart",
        "/properties/otelCollector/properties/Capabilities",
        "/properties/signoz/properties/Release",
        "/properties/clickhouse/properties/Release",
    ];
    for ptr in forbidden {
        assert!(
            schema.pointer(ptr).is_none(),
            "unexpected false-positive property exists at {ptr}"
        );
    }

    // Optional: compare coverage against a reference schema if provided.
    // This test is skipped unless SIGNOZ_SCHEMA_REF is set to a readable json file path.
    if let Ok(p) = std::env::var("SIGNOZ_SCHEMA_REF") {
        let ref_text = std::fs::read_to_string(&p)?;
        let ref_schema: serde_json::Value = serde_json::from_str(&ref_text)?;

        fn collect_prop_paths(schema: &serde_json::Value) -> std::collections::BTreeSet<String> {
            fn rec(
                node: &serde_json::Value,
                path: &mut Vec<String>,
                out: &mut std::collections::BTreeSet<String>,
            ) {
                let Some(o) = node.as_object() else {
                    return;
                };
                if let Some(props) = o.get("properties").and_then(|v| v.as_object()) {
                    for (k, v) in props {
                        path.push(k.clone());
                        out.insert(path.join("."));
                        rec(v, path, out);
                        path.pop();
                    }
                }
                if let Some(items) = o.get("items") {
                    path.push("*".to_string());
                    rec(items, path, out);
                    path.pop();
                }
            }

            let mut out = std::collections::BTreeSet::new();
            let mut path = Vec::new();
            rec(schema, &mut path, &mut out);
            out
        }

        let ours = collect_prop_paths(&schema);
        let theirs = collect_prop_paths(&ref_schema);

        // Avoid brittle full equality; instead enforce a minimum coverage ratio.
        // This should go up as we implement subsequent steps.
        let covered = ours.intersection(&theirs).count() as f64;
        let total = theirs.len().max(1) as f64;
        let ratio = covered / total;
        assert!(
            ratio >= 0.20,
            "coverage ratio too low vs reference: {ratio:.3} (covered {covered} / total {total})"
        );
    }

    Ok(())
}
