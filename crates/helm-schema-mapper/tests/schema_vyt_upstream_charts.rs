use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use test_util::prelude::*;
use vfs::VfsPath;

#[derive(Debug, serde::Deserialize)]
struct ChartYaml {
    name: String,
}

fn find_charts_by_name(dir: &VfsPath, wanted: &[&str]) -> eyre::Result<Vec<(String, VfsPath)>> {
    if !dir.exists()? {
        return Ok(vec![]);
    }

    let mut out: Vec<(String, VfsPath)> = Vec::new();
    let mut stack = vec![dir.clone()];

    while let Some(d) = stack.pop() {
        let chart_yaml = d.join("Chart.yaml")?;
        if chart_yaml.exists()? {
            let raw = chart_yaml.read_to_string()?;
            if let Ok(doc) = serde_yaml::from_str::<ChartYaml>(&raw) {
                if wanted.iter().any(|w| *w == doc.name) {
                    out.push((doc.name, d.clone()));
                }
            }
            continue;
        }

        for e in d.read_dir()? {
            if e.is_dir()? {
                stack.push(e);
            }
        }
    }

    Ok(out)
}

fn assert_any_pointer_type(schema: &serde_json::Value, cases: &[(&str, &str)]) -> eyre::Result<()> {
    for (ptr, ty) in cases {
        if let Some(v) = schema.pointer(ptr) {
            if v.as_str() == Some(*ty) {
                return Ok(());
            }
            if v.get("type").and_then(|t| t.as_str()) == Some(*ty) {
                return Ok(());
            }
            return Err(eyre::eyre!("pointer {ptr} exists but has unexpected type: {v}"));
        }
    }
    Err(eyre::eyre!("none of the candidate pointers existed: {cases:?}"))
}

#[test]
fn generates_values_schema_for_upstream_charts_if_present_vyt() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let charts_dir = root.join("crates/helm-schema-mapper/testdata/charts")?;

    let charts = find_charts_by_name(&charts_dir, &["redis", "cert-manager"])?;
    if charts.is_empty() {
        // Optional fixtures not present.
        return Ok(());
    }

    for (name, chart_root) in charts {
        let chart = load_chart(&chart_root, &LoadOptions::default())?;
        let schema = generate_values_schema_for_chart_vyt(&chart)?;

        // Sanity
        assert_eq!(
            schema.get("$schema").and_then(|v| v.as_str()),
            Some("http://json-schema.org/draft-07/schema#")
        );
        assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
        let props = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .ok_or_eyre("missing properties")?;
        assert!(!props.is_empty(), "schema properties unexpectedly empty for {name}");

        match name.as_str() {
            "cert-manager" => {
                // This is stable across cert-manager chart versions.
                let install_crds = schema
                    .pointer("/properties/installCRDs/type")
                    .ok_or_eyre("missing installCRDs.type")?;
                assert_eq!(install_crds.as_str(), Some("boolean"));

                // Often present; accept either top-level replicaCount or nested webhook/ controller.
                let _ = assert_any_pointer_type(
                    &schema,
                    &[
                        ("/properties/replicaCount/type", "integer"),
                        (
                            "/properties/cainjector/properties/replicaCount/type",
                            "integer",
                        ),
                        (
                            "/properties/webhook/properties/replicaCount/type",
                            "integer",
                        ),
                    ],
                );
            }
            "redis" => {
                // Stable in Bitnami redis.
                let auth_enabled = schema
                    .pointer("/properties/auth/properties/enabled/type")
                    .ok_or_eyre("missing auth.enabled.type")?;
                assert_eq!(auth_enabled.as_str(), Some("boolean"));

                // replicaCount might appear under different sections depending on chart version.
                let _ = assert_any_pointer_type(
                    &schema,
                    &[
                        ("/properties/replica/properties/replicaCount/type", "integer"),
                        ("/properties/replicaCount/type", "integer"),
                        ("/properties/master/properties/count/type", "integer"),
                    ],
                );
            }
            _ => {}
        }
    }

    Ok(())
}
