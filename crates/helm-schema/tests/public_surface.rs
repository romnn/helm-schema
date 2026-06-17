use color_eyre::eyre;
use helm_schema::AnalysisSession;
use helm_schema::generation::{GenerateOptions, generate_values_schema_for_chart};
use helm_schema::output::{JsonOutputFormat, OutputPipelineOptions, PolicyInputs, ReferenceMode};
use helm_schema::provider::{K8sVersionChain, ProviderOptions};
use serde_json::json;
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

#[test]
fn analysis_session_exposes_contract_and_generated_schema() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "replicas: 1\n")?;
    test_util::write(
        &chart_dir.join("templates/deployment.yaml")?,
        r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: root
spec:
  replicas: {{ .Values.replicas }}
"#,
    )?;

    let opts = GenerateOptions {
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
    };
    let session = AnalysisSession::new(opts);

    let analysis = session.analysis()?;
    assert!(
        !analysis.contract.clone().project().uses().is_empty(),
        "session contract should expose at least one use"
    );
    let generated = session.generated_schema()?;
    assert_eq!(
        generated
            .schema
            .pointer("/properties/replicas/type")
            .and_then(serde_json::Value::as_str),
        Some("integer")
    );

    Ok(())
}

#[test]
fn analysis_session_exposes_resolved_contract_before_required_inference() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;
    test_util::write(
        &chart_dir.join("values.schema.json")?,
        r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "mode": {
      "enum": ["safe", "fast"]
    }
  }
}"#,
    )?;
    test_util::write(
        &chart_dir.join("templates/serviceaccount.yaml")?,
        r#"
{{- if .Values.serviceAccount.create }}
apiVersion: v1
kind: ServiceAccount
metadata:
  name: root
{{- end }}
"#,
    )?;

    let session = AnalysisSession::new(GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: true,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            allow_net: false,
            disable_k8s_schemas: true,
            ..Default::default()
        },
    });

    let resolved = session.resolved_contract()?;
    assert_eq!(
        resolved.schema.pointer("/allOf/0/properties/mode/enum"),
        Some(&json!(["safe", "fast"]))
    );
    assert!(
        resolved
            .schema
            .pointer("/properties/serviceAccount/required")
            .is_none(),
        "resolved contract should stay pre-heuristic: {}",
        resolved.schema
    );

    let generated = session.generated_schema()?;
    assert_eq!(
        generated
            .schema
            .pointer("/properties/serviceAccount/required"),
        Some(&json!(["create"]))
    );
    assert_eq!(
        generated.schema.pointer("/allOf/0/properties/mode/enum"),
        Some(&json!(["safe", "fast"]))
    );

    Ok(())
}

#[test]
fn analysis_session_emits_final_schema_through_output_pipeline() -> eyre::Result<()> {
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

    let session = AnalysisSession::new(GenerateOptions {
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
    });

    let emitted = session.emit(
        PolicyInputs::default(),
        &OutputPipelineOptions {
            reference_mode: ReferenceMode::SelfContained,
            strip_descriptions: false,
            minimize: false,
        },
    )?;

    assert_eq!(
        emitted
            .get("x-helm-schema-generated")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        emitted
            .pointer("/properties/enabled/description")
            .and_then(serde_json::Value::as_str),
        Some("Whether the config map is enabled")
    );

    Ok(())
}

#[test]
fn analysis_session_explains_values_path() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc::indoc! {"
            apiVersion: v2
            name: root
            version: 0.1.0
            dependencies:
              - name: child
                alias: kid
                version: 0.1.0
                condition: kid.enabled, global.kidEnabled
                tags:
                  - observability
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;
    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "apiVersion: v2\nname: child\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        "enabled: true\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/configmap.yaml")?,
        r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: demo
data:
  enabled: "{{ .Values.enabled }}"
"#,
    )?;

    let session = AnalysisSession::new(GenerateOptions {
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
    });

    let explanation = session.explain("kid.enabled")?;

    assert_eq!(explanation.path, "kid.enabled");
    assert!(!explanation.exact_uses.is_empty(), "expected exact uses");
    assert!(
        explanation
            .exact_uses
            .iter()
            .any(|use_| use_.guards.contains(
                &serde_json::from_value(serde_json::json!({
                    "type": "or",
                    "paths": ["global.kidEnabled", "kid.enabled", "tags.observability"]
                }))
                .expect("deserialize guard")
            )),
        "expected activation guard in explanation: {explanation:#?}"
    );
    assert!(
        explanation
            .type_hints
            .iter()
            .any(|hint| hint == &json!({ "type": "boolean" })),
        "expected boolean activation type hint: {explanation:#?}"
    );

    Ok(())
}

#[test]
fn shipped_values_schema_is_enforced_as_constraint() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "mode: safe\n")?;
    test_util::write(
        &chart_dir.join("values.schema.json")?,
        r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "mode": {
      "enum": ["safe", "fast"]
    }
  }
}"#,
    )?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: root
data:
  mode: "{{ .Values.mode }}"
"#,
    )?;

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
        schema.pointer("/allOf/0/properties/mode/enum"),
        Some(&json!(["safe", "fast"]))
    );

    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&json!({ "mode": "safe" })));
    assert!(!validator.is_valid(&json!({ "mode": "unsafe" })));

    Ok(())
}

#[test]
fn generated_root_values_schema_is_not_reingested_as_shipped_constraint() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "mode: safe\n")?;
    test_util::write(
        &chart_dir.join("values.schema.json")?,
        r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "x-helm-schema-generated": true,
  "type": "object",
  "properties": {
    "mode": {
      "enum": ["generated-only"]
    }
  }
}"#,
    )?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: root
data:
  mode: "{{ .Values.mode }}"
"#,
    )?;

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

    assert!(
        schema.pointer("/allOf/0/properties/mode/enum").is_none(),
        "generated root schema must not be re-ingested as a shipped constraint"
    );

    Ok(())
}
