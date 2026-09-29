//! The caller authoring policy flows from `GenerateOptions` into generation
//! and into the policy annotation of both session emit paths.

use color_eyre::eyre;
use indoc::indoc;
use serde_json::json;
use vfs::VfsPath;

use test_util::prelude::sim_assert_eq;

use crate::generation::{AuthoringPolicy, DeclaredTypes, RootPolicy};
use crate::output::{
    EmitRequest, FetchPolicy, LoadBudget, OutputPipelineOptions, PolicyInputOptions,
    ReferencePolicy,
};

fn session(authoring: AuthoringPolicy) -> eyre::Result<crate::AnalysisSession> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: lone
            version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "replicas: 1\n")?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        // A custom kind with no resource schema: only the declared default
        // types `replicas`.
        indoc! {"
            apiVersion: example.com/v1
            kind: Widget
            metadata:
              name: lone
            spec:
              replicas: {{ .Values.replicas }}
        "},
    )?;
    Ok(crate::AnalysisSession::new(crate::GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: crate::generation::SchemaProfile::default().into(),
        authoring,
        provider: crate::provider::ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            ..Default::default()
        },
    }))
}

const REQUEST: EmitRequest = EmitRequest {
    reference_policy: ReferencePolicy::SelfContained,
    output: OutputPipelineOptions {
        strip_descriptions: false,
        minimize: false,
        definition_names: crate::output::DefinitionNames::Source,
    },
};

#[test]
fn both_emit_paths_record_the_policy_generation_used() -> eyre::Result<()> {
    let mut observed = Vec::new();
    for authoring in [
        AuthoringPolicy::default(),
        AuthoringPolicy {
            root: RootPolicy::Open,
            declared_types: DeclaredTypes::Annotate,
        },
    ] {
        let session = session(authoring)?;
        let emitted = session.emit(REQUEST)?;
        let with_paths = session.emit_with_policy_paths(
            &[],
            PolicyInputOptions {
                fetch_policy: FetchPolicy::input_assembly(false),
                load_budget: LoadBudget::default(),
            },
            REQUEST,
        )?;
        sim_assert_eq!(have: &emitted, want: &with_paths);
        let validator = jsonschema::validator_for(&emitted)?;
        observed.push((
            emitted["x-helm-schema-policy"]["authoring"].clone(),
            emitted.get("additionalProperties").cloned(),
            validator.is_valid(&json!({ "replicas": "three" })),
            validator.is_valid(&json!({ "unknown": true })),
        ));
    }
    sim_assert_eq!(
        have: observed,
        want: vec![
            (
                json!({"declared-types": "assert", "root": "closed"}),
                Some(json!(false)),
                false,
                false,
            ),
            (
                json!({"declared-types": "annotate", "root": "open"}),
                None,
                true,
                true,
            ),
        ]
    );
    Ok(())
}
