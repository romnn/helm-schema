//! Serialized values retain provider obligations and rendered YAML failure constraints.

use color_eyre::eyre;
use helm_schema::AnalysisSession;
use helm_schema::generation::{AuthoringPolicy, DeclaredTypes, GenerateOptions, SchemaProfile};
use helm_schema::output::{EmitRequest, OutputPipelineOptions, ReferencePolicy};
use helm_schema::provider::ProviderOptions;
use indoc::indoc;
use serde_json::Value;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

// Execute the actual helper body, including its scope branch, rather than recognizing its name.
const RENDER: &str = indoc! {r#"
    {{- define "common.tplvalues.render" -}}
    {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) }}
    {{- if contains "{{" (toJson .value) }}
      {{- if .scope }}
          {{- tpl (cat "{{- with $.RelativeScope -}}" $value "{{- end }}") (merge (dict "RelativeScope" .scope) .context) }}
      {{- else }}
        {{- tpl $value .context }}
      {{- end }}
    {{- else }}
        {{- $value }}
    {{- end }}
    {{- end -}}
"#};

fn generate(template: &str, values: &str, provider_enabled: bool) -> eyre::Result<Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: serialized-output
            version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, values)?;
    test_util::write(&chart_dir.join("templates/resource.yaml")?, template)?;
    test_util::write(&chart_dir.join("templates/_helpers.tpl")?, RENDER)?;
    let bundle = test_util::workspace_testdata().join("provider-bundle");
    let session = AnalysisSession::new(GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: false,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::Full.into(),
        authoring: AuthoringPolicy {
            declared_types: DeclaredTypes::Annotate,
            ..Default::default()
        },
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(bundle.join("kubernetes-json-schema-cache")),
            crd_catalog_cache_dir: Some(bundle.join("crds-catalog-cache")),
            crd_override_dir: Some(bundle.join("crds-catalog-cache")),
            allow_net: false,
            disable_k8s_schemas: !provider_enabled,
            ..Default::default()
        },
    });
    Ok(session.emit(EmitRequest {
        reference_policy: ReferencePolicy::SelfContained,
        output: OutputPipelineOptions {
            strip_descriptions: true,
            minimize: false,
            definition_names: helm_schema::output::DefinitionNames::Source,
        },
    })?)
}

#[test]
fn redis_affinity_survives_common_tplvalues_render() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    // Freeze unrelated Redis gates; retain the affinity truthiness bypass and helper call.
    let have = generate(
        indoc! {r#"
            apiVersion: apps/v1
            kind: StatefulSet
            metadata:
              name: redis
            spec:
              serviceName: redis
              selector:
                matchLabels:
                  app: redis
              template:
                metadata:
                  labels:
                    app: redis
                spec:
                  {{- if .Values.replica.affinity }}
                  affinity: {{- include "common.tplvalues.render" (dict "value" .Values.replica.affinity "context" $) | nindent 8 }}
                  {{- end }}
                  containers:
                    - name: redis
                      image: redis
        "#},
        "replica: {}\n",
        true,
    )?;
    // The complete provider object arm must survive alongside falsey and runtime text arms.
    let want: Value = serde_json::from_str(include_str!(
        "fixtures/serialized_output/redis-affinity.json"
    ))?;
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

#[test]
fn argo_applicationset_resources_survives_to_yaml() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let have = generate(
        indoc! {r"
            apiVersion: apps/v1
            kind: Deployment
            metadata:
              name: applicationset
            spec:
              selector:
                matchLabels:
                  app: applicationset
              template:
                metadata:
                  labels:
                    app: applicationset
                spec:
                  containers:
                    - name: applicationset
                      image: argocd
                      resources:
                        {{- toYaml .Values.applicationSet.resources | nindent 12 }}
        "},
        "applicationSet: {}\n",
        true,
    )?;
    // Claims, limits and requests are the full pinned Deployment resource contract.
    let want: Value = serde_json::from_str(include_str!(
        "fixtures/serialized_output/argo-resources.json"
    ))?;
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

#[test]
fn cilium_affinity_preserves_scheduling_guard() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let have = generate(
        indoc! {r#"
            apiVersion: apps/v1
            kind: DaemonSet
            metadata:
              name: cilium
            spec:
              selector:
                matchLabels:
                  app: cilium
              template:
                metadata:
                  labels:
                    app: cilium
                spec:
                  {{- if (eq .Values.scheduling.mode "anti-affinity") }}
                  {{- with .Values.affinity }}
                  affinity:
                    {{- toYaml . | nindent 8 }}
                  {{- end }}
                  {{- end }}
                  containers:
                    - name: cilium
                      image: cilium
        "#},
        indoc! {"
            scheduling: {}
            affinity: null
        "},
        true,
    )?;
    // Outside anti-affinity mode the whole affinity value remains unconstrained.
    let want: Value = serde_json::from_str(include_str!(
        "fixtures/serialized_output/cilium-affinity.json"
    ))?;
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

#[test]
fn redis_common_annotations_records_helm_decode_failure() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: redis-configuration
          {{- if .Values.commonAnnotations }}
          annotations: {{- include "common.tplvalues.render" (dict "value" .Values.commonAnnotations "context" $) | nindent 4 }}
          {{- end }}
        data:
          redis.conf: ""
    "#};
    let want: Value = serde_json::from_str(include_str!(
        "fixtures/serialized_output/redis-annotations.json"
    ))?;
    // Helm decodes metadata even when Kubernetes provider validation is disabled.
    // Evaluate both modes before asserting so the red run records independent failure coverage.
    let without_provider = generate(template, "commonAnnotations: null\n", false)?;
    let with_provider = generate(template, "commonAnnotations: null\n", true)?;
    let without_provider_want: Value = serde_json::from_str(include_str!(
        "fixtures/serialized_output/redis-annotations-disabled.json"
    ))?;
    sim_assert_eq!(have: [&without_provider, &with_provider], want: [&without_provider_want, &want]);
    Ok(())
}

#[test]
fn argo_crd_additional_labels_requires_mapping_contribution() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let template = indoc! {r"
        {{- if .Values.crds.install }}
        apiVersion: apiextensions.k8s.io/v1
        kind: CustomResourceDefinition
        metadata:
          labels:
            app.kubernetes.io/name: applications.argoproj.io
            app.kubernetes.io/part-of: argocd
            {{- with .Values.crds.additionalLabels }}
              {{- toYaml . | nindent 4 }}
            {{- end }}
          name: applications.argoproj.io
        spec:
          group: argoproj.io
          names:
            kind: Application
            plural: applications
          scope: Namespaced
          versions:
            - name: v1alpha1
              served: true
              storage: true
              schema:
                openAPIV3Schema:
                  type: object
        {{- end }}
    "};
    let want: Value =
        serde_json::from_str(include_str!("fixtures/serialized_output/argo-labels.json"))?;
    // A truthy scalar spliced among sibling labels fails YAML parsing without a provider.
    let without_provider = generate(template, "crds: {}\n", false)?;
    let with_provider = generate(template, "crds: {}\n", true)?;
    sim_assert_eq!(have: [&without_provider, &with_provider], want: [&want, &want]);
    Ok(())
}
