//! Whole-chart regression for the un-spaced pipe misparse (v0.0.6): the
//! tree-sitter grammar absorbed `toYaml .| nindent 8`'s pipe into the
//! argument, splitting `nindent 8` apart and fabricating an unsatisfiable
//! "subject must be a string" clause at the schema root. Pipe spacing must
//! never change the generated schema, and the schema must accept the values
//! Helm renders.

use std::path::Path;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema::generation::{GenerateOptions, SchemaProfile};
use helm_schema::output::{EmitRequest, OutputPipelineOptions, ReferencePolicy};
use helm_schema::provider::ProviderOptions;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

const CHART_YAML: &str = indoc! {"
    apiVersion: v2
    name: repro
    version: 0.1.0
"};

const VALUES_YAML: &str = indoc! {"
    podLabels: {}
    podAnnotations: {}
"};

/// The report's two trigger shapes: the cursor form inside `with`, and the
/// bare field-path form. `__SP__` replaces the WHOLE separator between the
/// piped subject and `nindent`, so the `"|"` spacing exercises the fully
/// un-spaced `.|nindent` spelling.
fn job_template(spacing: &str) -> String {
    indoc! {"
        apiVersion: batch/v1
        kind: Job
        metadata:
          name: repro
          annotations:
            {{- toYaml .Values.podAnnotations__SP__nindent 4 }}
        spec:
          template:
            metadata:
              labels:
                {{- with .Values.podLabels }}
                {{- toYaml .__SP__nindent 8 }}
                {{- end }}
            spec:
              restartPolicy: Never
              containers:
                - name: c
                  image: busybox
    "}
    .replace("__SP__", spacing)
}

fn write_chart(dir: &Path, template: &str) -> eyre::Result<()> {
    std::fs::write(dir.join("Chart.yaml"), CHART_YAML).wrap_err("write Chart.yaml")?;
    std::fs::write(dir.join("values.yaml"), VALUES_YAML).wrap_err("write values.yaml")?;
    let templates = dir.join("templates");
    std::fs::create_dir_all(&templates).wrap_err("create templates dir")?;
    std::fs::write(templates.join("job.yaml"), template).wrap_err("write job.yaml")?;
    Ok(())
}

fn emit_schema(chart_dir: &Path) -> eyre::Result<Value> {
    let chart_dir = chart_dir.to_string_lossy().to_string();
    let session = helm_schema::AnalysisSession::new(GenerateOptions {
        chart_dir: VfsPath::new(vfs::PhysicalFS::new(&chart_dir)),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::Full.into(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(
                test_util::workspace_testdata()
                    .join("provider-bundle/kubernetes-json-schema-cache"),
            ),
            allow_net: false,
            crd_catalog_cache_dir: Some(
                test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
            ),
            disable_k8s_schemas: false,
            crd_override_dir: Some(
                test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
            ),
            ..Default::default()
        },
    });
    Ok(session.emit(EmitRequest {
        reference_policy: ReferencePolicy::SelfContained,
        output: OutputPipelineOptions {
            strip_descriptions: false,
            minimize: true,
        },
    })?)
}

fn schema_for_spacing(spacing: &str) -> eyre::Result<Value> {
    let tempdir = tempfile::tempdir().wrap_err("create chart directory")?;
    write_chart(tempdir.path(), &job_template(spacing))?;
    emit_schema(tempdir.path())
}

#[test]
fn pipe_spacing_never_changes_the_generated_schema() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let want = schema_for_spacing(" | ")?;
    for spacing in ["|", " |", "| "] {
        let have = schema_for_spacing(spacing)?;
        sim_assert_eq!(have: &have, want: &want, "spacing {spacing:?} changed the schema");
    }
    Ok(())
}

#[test]
fn unspaced_pipe_chart_accepts_the_values_helm_renders() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = schema_for_spacing("|")?;
    let validator = jsonschema::validator_for(&schema)?;

    // The chart's own defaults and the overlays Helm renders byte-identically
    // to the spaced form must validate.
    let accepted = [
        json!({ "podLabels": {}, "podAnnotations": {} }),
        json!({ "podLabels": { "app": "demo" }, "podAnnotations": {} }),
        json!({
            "podLabels": { "azure.workload.identity/use": "true" },
            "podAnnotations": { "checksum": "abc" },
        }),
    ];
    for instance in &accepted {
        let errors: Vec<String> = validator
            .iter_errors(instance)
            .map(|error| error.to_string())
            .collect();
        sim_assert_eq!(have: &errors, want: &Vec::<String>::new(), "rejected {instance}");
    }

    // A scalar where the label map belongs still fails: the fix must not
    // degrade the sink typing into "anything goes".
    sim_assert_eq!(
        have: validator.is_valid(&json!({ "podLabels": "text", "podAnnotations": {} })),
        want: false
    );
    Ok(())
}
