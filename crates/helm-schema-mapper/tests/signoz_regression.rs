use color_eyre::eyre;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use test_util::prelude::*;
use vfs::VfsPath;

fn ptr<'a>(schema: &'a serde_json::Value, p: &str) -> Option<&'a serde_json::Value> {
    schema.pointer(p)
}

fn assert_any_exists(schema: &serde_json::Value, candidates: &[&str]) -> eyre::Result<()> {
    for c in candidates {
        if ptr(schema, c).is_some() {
            return Ok(());
        }
    }
    Err(eyre::eyre!("none of the candidate pointers exist: {candidates:?}"))
}

fn assert_type(schema: &serde_json::Value, p: &str, ty: &str) -> eyre::Result<()> {
    let v = schema
        .pointer(p)
        .ok_or_else(|| eyre::eyre!("missing pointer: {p}"))?;
    eyre::ensure!(v.as_str() == Some(ty), "expected {p} == {ty:?}, got {v}");
    Ok(())
}

fn assert_string_map(schema: &serde_json::Value, p: &str) -> eyre::Result<()> {
    let ap = schema
        .pointer(p)
        .and_then(|v| v.get("additionalProperties"))
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| eyre::eyre!("missing {p}.additionalProperties.type"))?;
    eyre::ensure!(ap == "string", "expected string map at {p}, got {ap:?}");
    Ok(())
}

fn assert_path_type_enum_or_string(schema: &serde_json::Value, base: &str) -> eyre::Result<()> {
    let enum_ptr = format!("{base}/enum");
    if let Some(arr) = schema.pointer(&enum_ptr).and_then(|v| v.as_array()) {
        eyre::ensure!(
            arr.iter().any(|v| v.as_str() == Some("ImplementationSpecific")),
            "{enum_ptr} did not contain ImplementationSpecific"
        );
        return Ok(());
    }

    let type_ptr = format!("{base}/type");
    assert_type(schema, &type_ptr, "string")
}

#[test]
fn signoz_regression_suite() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_root = root.join("crates/helm-schema-mapper/testdata/charts/signoz-signoz")?;
    if !chart_root.exists()? {
        return Ok(());
    }

    let chart = load_chart(&chart_root, &LoadOptions::default())?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    // Presence checks
    assert_any_exists(&schema, &["/properties/clusterName"]) ?;
    assert_any_exists(&schema, &["/properties/signoz"]) ?;
    assert_any_exists(&schema, &["/properties/clickhouse"]) ?;
    assert_any_exists(&schema, &["/properties/signoz-otel-gateway"]) ?;

    // Type checks (representative)
    assert_type(&schema, "/properties/clusterName/type", "string")?;
    assert_type(
        &schema,
        "/properties/signoz/properties/ingress/properties/enabled/type",
        "boolean",
    )?;
    assert_type(
        &schema,
        "/properties/signoz-otel-gateway/properties/replicaCount/type",
        "integer",
    )?;

    // string-map-ish
    assert_string_map(
        &schema,
        "/properties/signoz/properties/ingress/properties/annotations",
    )?;

    // Depth checks (representative)
    assert_path_type_enum_or_string(
        &schema,
        "/properties/alertmanager/properties/ingress/properties/hosts/items/properties/paths/items/properties/pathType",
    )
    .or_else(|_| {
        assert_path_type_enum_or_string(
            &schema,
            "/properties/signoz/properties/alertmanager/properties/ingress/properties/hosts/items/properties/paths/items/properties/pathType",
        )
    })?;

    assert_any_exists(
        &schema,
        &[
            "/properties/clickhouse/properties/clickhouseOperator/properties/image/properties/repository/type",
            "/properties/signoz/properties/clickhouse/properties/clickhouseOperator/properties/image/properties/repository/type",
        ],
    )?;

    assert_any_exists(
        &schema,
        &[
            "/properties/signoz/properties/smtpVars/properties/existingSecret/properties/usernameKey/type",
            "/properties/smtpVars/properties/existingSecret/properties/usernameKey/type",
        ],
    )?;

    // Keep Step 3 noise invariants in this suite too.
    for ptr in [
        "/properties/otelCollector/properties/Release",
        "/properties/otelCollector/properties/Chart",
        "/properties/otelCollector/properties/Capabilities",
        "/properties/signoz/properties/Release",
        "/properties/clickhouse/properties/Release",
    ] {
        eyre::ensure!(schema.pointer(ptr).is_none(), "unexpected property exists at {ptr}");
    }

    // Optional reference compare.
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
                if let Some(ap) = o.get("additionalProperties") {
                    if ap.is_object() {
                        path.push("__any__".to_string());
                        rec(ap, path, out);
                        path.pop();
                    }
                }
            }

            let mut out = std::collections::BTreeSet::new();
            let mut path = Vec::new();
            rec(schema, &mut path, &mut out);
            out
        }

        let ours = collect_prop_paths(&schema);
        let theirs = collect_prop_paths(&ref_schema);

        let covered = ours.intersection(&theirs).count() as f64;
        let total = theirs.len().max(1) as f64;
        let ratio = covered / total;

        eyre::ensure!(
            ratio >= 0.30,
            "coverage ratio too low vs reference: {ratio:.3} (covered {covered} / total {total})"
        );
    }

    Ok(())
}
