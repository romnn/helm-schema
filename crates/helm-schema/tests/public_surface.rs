use color_eyre::eyre;
use helm_schema::generation::{
    GenerateOptions, generate_values_schema_for_chart, generate_values_schema_for_chart_output,
};
use helm_schema::output::{JsonOutputFormat, OutputPipelineOptions, PolicyInputs, ReferenceMode};
use helm_schema::provider::{K8sVersionChain, ProviderOptions};
use helm_schema::{
    AnalysisSession, CliError, contract::ContractDocument, diagnostics::DiagnosticSink,
};
use helm_schema_engine::Guard;
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
    assert!(
        analysis
            .contract
            .clone()
            .project()
            .uses()
            .iter()
            .any(|use_| !use_.provenance.is_empty()),
        "session contract uses should now retain source provenance"
    );
    let contract_document = session.contract_document()?;
    assert_eq!(contract_document.version, 2);
    assert!(
        !contract_document.uses.is_empty(),
        "session contract document should expose canonical uses"
    );
    assert!(
        contract_document
            .uses
            .iter()
            .any(|use_| !use_.provenance.is_empty()),
        "session contract document should retain source provenance"
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
fn contract_document_is_byte_deterministic_across_100_runs() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        indoc::indoc! {"
            enabled: true
            message: hello
            replicas: 2
        "},
    )?;
    test_util::write(
        &chart_dir.join("templates/_helpers.tpl")?,
        indoc::indoc! {r#"
            {{- define "root.renderMessage" -}}
            {{- if .Values.enabled -}}
            {{ .Values.message }}
            {{- else -}}
            fallback
            {{- end -}}
            {{- end -}}
        "#},
    )?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        indoc::indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: root
            data:
              message: {{ include "root.renderMessage" . | quote }}
              replicas: {{ .Values.replicas | quote }}
        "#},
    )?;

    let opts = GenerateOptions {
        chart_dir: chart_dir.clone(),
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

    let expected = serde_json::to_vec(&AnalysisSession::new(opts.clone()).contract_document()?)?;
    for _ in 0..100 {
        let actual = serde_json::to_vec(&AnalysisSession::new(opts.clone()).contract_document()?)?;
        assert_eq!(actual, expected, "contract DTO bytes must be deterministic");
    }

    Ok(())
}

#[test]
fn stage_functions_match_session_generated_schema() -> eyre::Result<()> {
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

    let staged = generate_values_schema_for_chart_output(&opts, None)?;
    let session = AnalysisSession::new(opts);
    let session_generated = session.generated_schema()?;

    similar_asserts::assert_eq!(staged.schema, session_generated.schema);
    similar_asserts::assert_eq!(
        staged.subchart_value_prefixes,
        session_generated.subchart_value_prefixes
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
    assert_eq!(
        explanation.exact_uses.len(),
        explanation.exact_use_sites.len(),
        "provenance-aware rows should stay aligned with compatibility rows"
    );
    assert!(
        explanation
            .exact_uses
            .iter()
            .any(|use_| use_.guards.contains(&Guard::Or {
                paths: vec![
                    "global.kidEnabled".to_string(),
                    "kid.enabled".to_string(),
                    "tags.observability".to_string(),
                ],
            })),
        "expected activation guard in explanation: {explanation:#?}"
    );
    assert!(
        explanation
            .type_hints
            .iter()
            .any(|hint| hint == &json!({ "type": "boolean" })),
        "expected boolean activation type hint: {explanation:#?}"
    );
    assert!(
        explanation.exact_use_sites.iter().any(|use_site| {
            use_site.provenance.iter().any(|provenance| {
                provenance
                    .template_path
                    .contains("templates/configmap.yaml")
                    && provenance.span.start < provenance.span.end
            })
        }),
        "expected source-file provenance for the explained kid.enabled use: {explanation:#?}"
    );

    Ok(())
}

#[test]
fn contract_document_json_round_trip_preserves_provenance_and_guards() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        "enabled: true\nmessage: hello\n",
    )?;
    test_util::write(
        &chart_dir.join("templates/_helpers.tpl")?,
        indoc::indoc! {r#"
            {{- define "root.renderMessage" -}}
            {{- if .Values.enabled -}}
            {{ .Values.message }}
            {{- else -}}
            fallback
            {{- end -}}
            {{- end -}}
        "#},
    )?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        indoc::indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: root
            data:
              message: {{ include "root.renderMessage" . | quote }}
        "#},
    )?;

    let session = AnalysisSession::with_diagnostics(
        GenerateOptions {
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
        },
        DiagnosticSink::new(),
    );

    let document = session.contract_document()?;
    let json = serde_json::to_value(&document)?;
    let decoded: ContractDocument = serde_json::from_value(json)?;

    assert_eq!(decoded, document);
    assert!(
        decoded
            .uses
            .iter()
            .any(|use_| !use_.provenance.is_empty() && !use_.value_use.guards.is_empty()),
        "round-tripped v2 document should retain provenance and guards"
    );

    Ok(())
}

#[test]
fn analysis_session_explains_helper_origin_provenance() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "message: hello\n")?;
    test_util::write(
        &chart_dir.join("templates/_helpers.tpl")?,
        r#"
{{- define "root.renderMessage" -}}
{{ .Values.message }}
{{- end -}}
"#,
    )?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: root
data:
  message: {{ include "root.renderMessage" . }}
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

    let explanation = session.explain("message")?;

    assert!(
        !explanation.exact_use_sites.is_empty(),
        "expected exact uses"
    );
    assert!(
        explanation.exact_use_sites.iter().any(|use_site| {
            use_site.provenance.iter().any(|provenance| {
                provenance.template_path.contains("templates/_helpers.tpl")
                    && provenance.span.start < provenance.span.end
                    && provenance.helper_chain == vec!["root.renderMessage".to_string()]
            })
        }),
        "expected helper-body provenance for message: {explanation:#?}"
    );

    Ok(())
}

#[test]
fn dependency_activation_guards_lower_to_root_any_of_condition() -> eyre::Result<()> {
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
    test_util::write(&chart_dir.join("charts/child/values.yaml")?, "{}\n")?;
    test_util::write(
        &chart_dir.join("charts/child/templates/configmap.yaml")?,
        r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: "{{ .Values.name }}"
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
        schema.pointer("/allOf/0/if/anyOf/0/properties/global/properties/kidEnabled/const"),
        Some(&json!(true))
    );
    assert_eq!(
        schema.pointer("/allOf/0/if/anyOf/1/properties/kid/properties/enabled/const"),
        Some(&json!(true))
    );
    assert_eq!(
        schema.pointer("/allOf/0/if/anyOf/2/properties/tags/properties/observability/const"),
        Some(&json!(true))
    );
    assert_eq!(
        schema.pointer("/allOf/0/then/properties/kid/properties/name/type"),
        Some(&json!("string")),
        "subchart values path should be guarded by Chart.yaml activation predicates: {schema}"
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

#[test]
fn resolved_contract_rejects_invalid_composed_defaults() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "mode: unsafe\n")?;
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

    let err = session
        .resolved_contract()
        .expect_err("invalid shipped defaults constraint should fail postcondition");
    match err {
        CliError::SchemaPostconditionViolated { errors } => {
            assert!(
                errors.iter().any(|err| err.contains("/mode")),
                "expected path-specific validation error, got {errors:?}"
            );
        }
        other => panic!("expected schema postcondition error, got {other:?}"),
    }

    Ok(())
}
