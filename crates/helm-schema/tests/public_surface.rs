use color_eyre::eyre;
use helm_schema::generation::GenerateOptions;
use helm_schema::output::{JsonOutputFormat, OutputPipelineOptions, PolicyInputs, ReferenceMode};
use helm_schema::provider::{K8sVersionChain, ProviderOptions};
use helm_schema::{
    AnalysisSession,
    contract::{ContractDocument, Guard, ValueKind},
    diagnostics::DiagnosticSink,
};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

fn generate_values_schema_for_chart(
    opts: &GenerateOptions,
) -> helm_schema::EngineResult<serde_json::Value> {
    AnalysisSession::new(opts.clone())
        .generated_schema()
        .map(|generated| generated.schema)
}

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
    sim_assert_eq!(have: versions, want: vec!["v1.35.0".to_string(), "v1.34.0".to_string()]);
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

    sim_assert_eq!(
        have: schema,
        want: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "enabled": {
                    "description": "Whether the config map is enabled",
                    "type": ["boolean", "integer", "number", "string"]
                }
            },
            "type": "object"
        })
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
        !analysis.contract.clone().finalize().uses().is_empty(),
        "session contract should expose at least one use"
    );
    assert!(
        analysis
            .contract
            .clone()
            .finalize()
            .uses()
            .iter()
            .any(|use_| !use_.provenance.is_empty()),
        "session contract uses should now retain source provenance"
    );
    let contract_document = session.contract_document()?;
    sim_assert_eq!(have: contract_document.version, want: 3);
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
    let signals = session.contract_schema_signals()?;
    assert!(
        signals.evidence_for("replicas").is_some(),
        "session schema-signal query should expose path-local evidence"
    );
    let generated = session.generated_schema()?;
    sim_assert_eq!(
        have: generated
            .schema
            .pointer("/properties/replicas/type")
            .and_then(serde_json::Value::as_str),
        want: Some("integer")
    );

    Ok(())
}

#[test]
fn deployment_security_context_fragments_keep_nested_provider_paths() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        indoc::indoc! {"
            web:
              enabled: true
              containerSecurityContext: {}
              securityContext: {}
        "},
    )?;
    test_util::write(
        &chart_dir.join("templates/deployment.yaml")?,
        indoc::indoc! {r#"
            {{- if .Values.web.enabled -}}
            apiVersion: apps/v1
            kind: Deployment
            metadata:
              name: root
            spec:
              template:
                spec:
                  containers:
                    - name: web
                      image: example
                      {{- with .Values.web.containerSecurityContext }}
                      securityContext:
                        {{- toYaml . | nindent 12 }}
                      {{- end }}
                  {{- with .Values.web.securityContext }}
                  securityContext:
                    {{- toYaml . | nindent 8 }}
                  {{- end }}
            {{- end }}
        "#},
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
            ..Default::default()
        },
    });

    let contract = session.contract_document()?;
    // A direct `toYaml . | nindent` splice renders any input kind, so the
    // placed row keeps its structural attachment (which the provider types)
    // without a structured-input shape claim.
    assert!(
        contract.uses.iter().any(|use_| {
            use_.source_expr == "web.containerSecurityContext"
                && use_.kind == ValueKind::YamlSerialized
                && use_.path.0
                    == [
                        "spec".to_string(),
                        "template".to_string(),
                        "spec".to_string(),
                        "containers[*]".to_string(),
                        "securityContext".to_string(),
                    ]
        }),
        "expected containerSecurityContext to render at container securityContext; uses={:#?}",
        contract.uses
    );

    let schema = session.generated_schema()?.schema;
    let web_schema = schema
        .pointer("/properties/web")
        .unwrap_or_else(|| panic!("missing web schema: {schema}"));
    assert!(
        !contains_required_property(web_schema, "containers"),
        "web schema must not require PodSpec containers under security contexts: {web_schema}"
    );

    Ok(())
}

fn contains_required_property(schema: &Value, property: &str) -> bool {
    match schema {
        Value::Object(object) => {
            object
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| {
                    required
                        .iter()
                        .any(|value| value.as_str() == Some(property))
                })
                || object
                    .values()
                    .any(|value| contains_required_property(value, property))
        }
        Value::Array(values) => values
            .iter()
            .any(|value| contains_required_property(value, property)),
        _ => false,
    }
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
        sim_assert_eq!(have: actual, want: expected, "contract document bytes must be deterministic");
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

    let session = AnalysisSession::new(opts);
    let staged = session.generated_schema()?;
    let session_generated = session.generated_schema()?;

    sim_assert_eq!(have: staged.schema, want: session_generated.schema);
    sim_assert_eq!(
        have: staged.subchart_value_prefixes,
        want: session_generated.subchart_value_prefixes
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
    test_util::write(
        &chart_dir.join("values.yaml")?,
        "mode: unsafe\nserviceAccount:\n  create: false\n",
    )?;
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
{{- if .Values.serviceAccount.create }}
apiVersion: v1
kind: ConfigMap
metadata:
  name: root
data:
  mode: "{{ .Values.mode }}"
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
    sim_assert_eq!(
        have: &resolved.schema,
        want: &json!({
            "$defs": {
                "helm-truthy": {
                    "anyOf": [
                        { "const": true },
                        { "not": { "const": 0 }, "type": "number" },
                        { "minLength": 1, "type": "string" },
                        { "minItems": 1, "type": "array" },
                        { "minProperties": 1, "type": "object" }
                    ]
                }
            },
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [{
                "if": {
                    "properties": {
                        "serviceAccount": {
                            "properties": {
                                "create": { "$ref": "#/$defs/helm-truthy" }
                            },
                            "required": ["create"],
                            "type": "object"
                        }
                    },
                    "required": ["serviceAccount"],
                    "type": "object"
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "mode": {
                            "anyOf": [
                                { "type": "boolean" },
                                { "type": "integer" },
                                { "type": "number" },
                                { "type": "string" }
                            ]
                        }
                    }
                }
            }],
            "properties": {
                "mode": {},
                "serviceAccount": {
                    "additionalProperties": {},
                    "properties": {
                        "create": {
                            "anyOf": [
                                { "not": { "$ref": "#/$defs/helm-truthy" } },
                                { "type": "boolean" }
                            ]
                        }
                    },
                    "type": "object"
                }
            },
            "type": "object"
        })
    );

    let generated = session.generated_schema()?;
    sim_assert_eq!(have: generated.schema, want: resolved.schema);

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

    sim_assert_eq!(
        have: emitted
            .get("x-helm-schema-generated")
            .and_then(serde_json::Value::as_bool),
        want: Some(true)
    );
    sim_assert_eq!(
        have: emitted
            .pointer("/properties/enabled/description")
            .and_then(serde_json::Value::as_str),
        want: Some("Whether the config map is enabled")
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

    sim_assert_eq!(have: explanation.path, want: "kid.enabled");
    assert!(!explanation.exact_uses.is_empty(), "expected exact uses");
    assert!(
        explanation
            .exact_uses
            .iter()
            .any(
                |use_| use_.condition.guard_conjunctions().iter().any(|guards| {
                    guards.as_slice()
                        == [Guard::Truthy {
                            path: "kid.enabled".to_string(),
                        }]
                        .as_slice()
                })
            ),
        "expected first condition activation branch in explanation: {explanation:#?}"
    );
    assert!(
        explanation
            .exact_uses
            .iter()
            .any(
                |use_| use_.condition.guard_conjunctions().iter().any(|guards| {
                    guards.as_slice()
                        == [
                            Guard::Truthy {
                                path: "global.kidEnabled".to_string(),
                            },
                            Guard::Absent {
                                path: "kid.enabled".to_string(),
                            },
                        ]
                        .as_slice()
                })
            ),
        "expected second condition branch guarded by first-condition absence: {explanation:#?}"
    );
    assert!(
        explanation
            .type_hints
            .iter()
            .any(|hint| hint == &json!({ "type": "boolean" })),
        "expected boolean activation type hint: {explanation:#?}"
    );
    assert!(
        explanation.exact_uses.iter().any(|use_site| {
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

    sim_assert_eq!(have: decoded, want: document);
    assert!(
        decoded
            .uses
            .iter()
            .any(|use_| !use_.provenance.is_empty() && !use_.condition.is_unconditional()),
        "round-tripped contract document should retain provenance and guards"
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

    assert!(!explanation.exact_uses.is_empty(), "expected exact uses");
    assert!(
        explanation.exact_uses.iter().any(|use_site| {
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
fn dependency_activation_guards_lower_with_helm_precedence() -> eyre::Result<()> {
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

    let validator = jsonschema::validator_for(&schema)?;
    assert!(
        !validator.is_valid(&json!({ "kid": { "enabled": true, "name": 7 } })),
        "first condition true should activate subchart schema: {schema}"
    );
    assert!(
        validator.is_valid(&json!({ "kid": { "enabled": false, "name": 7 } })),
        "first condition false should disable subchart schema even if name is invalid: {schema}"
    );
    assert!(
        !validator.is_valid(&json!({
            "global": { "kidEnabled": true },
            "kid": { "name": 7 }
        })),
        "second condition true should activate when first condition is absent: {schema}"
    );
    assert!(
        validator.is_valid(&json!({
            "global": { "kidEnabled": true },
            "kid": { "enabled": false, "name": 7 }
        })),
        "first condition false should override a later true condition: {schema}"
    );
    assert!(
        !validator.is_valid(&json!({
            "tags": { "observability": true },
            "kid": { "name": 7 }
        })),
        "tag true should activate when all conditions are absent: {schema}"
    );
    assert!(
        validator.is_valid(&json!({
            "tags": { "observability": false },
            "kid": { "name": 7 }
        })),
        "tag false should disable when all conditions are absent: {schema}"
    );
    assert!(
        !validator.is_valid(&json!({ "kid": { "name": 7 } })),
        "dependency should remain active by default when no activation values exist: {schema}"
    );

    Ok(())
}

#[test]
fn sibling_values_schema_file_is_not_inference_evidence() -> eyre::Result<()> {
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

    sim_assert_eq!(
        have: &schema,
        want: &json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "mode": {
                    "type": ["boolean", "integer", "number", "string"]
                }
            },
            "type": "object"
        })
    );

    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&json!({ "mode": "safe" })));
    assert!(validator.is_valid(&json!({ "mode": "unsafe" })));

    Ok(())
}
