use color_eyre::eyre;
use helm_schema::generation::{GenerateOptions, generate_values_schema_for_chart};
use helm_schema::output::{JsonOutputFormat, ReferenceMode};
use helm_schema::provider::{K8sVersionChain, ProviderOptions};
use vfs::VfsPath;

#[test]
fn facade_generates_schema_for_memory_chart() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        "# -- Whether the config map is enabled\nenabled: true\n",
    )?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: root
data:
  enabled: "{{ .Values.enabled }}"
"#,
    )?;

    let versions = K8sVersionChain::new(vec!["v1.35.0".to_string()], Some(1)).ordered();
    assert_eq!(versions, vec!["v1.35.0".to_string(), "v1.34.0".to_string()]);
    assert!(matches!(
        JsonOutputFormat::from_compact(false),
        JsonOutputFormat::Pretty
    ));
    assert!(matches!(
        ReferenceMode::from_flags(false, false),
        ReferenceMode::SelfContained
    ));

    let schema = generate_values_schema_for_chart(&GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            allow_net: false,
            disable_k8s_schemas: true,
            ..Default::default()
        },
    })?;

    assert_eq!(
        schema
            .pointer("/properties/enabled/type")
            .and_then(serde_json::Value::as_str),
        Some("boolean")
    );
    assert_eq!(
        schema
            .pointer("/properties/enabled/description")
            .and_then(serde_json::Value::as_str),
        Some("Whether the config map is enabled")
    );

    Ok(())
}
