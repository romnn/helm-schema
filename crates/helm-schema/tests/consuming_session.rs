//! Verifies consuming sessions preserve generated schemas and final output policy.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema::generation::{AuthoringPolicy, SchemaProfile};
use helm_schema::output::{
    DefinitionNames, EmitRequest, FetchPolicy, LoadBudget, OutputPipelineOptions,
    PolicyInputOptions, ReferencePolicy, serialize_default_schema_json,
};
use helm_schema::provider::ProviderOptions;
use helm_schema::{AnalysisSession, CliError, GenerateOptions, GeneratedSchema};
use indoc::indoc;
use serde_json::json;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;
use vfs::VfsPath;

fn generate_options(chart_dir: VfsPath, infer_required: bool) -> GenerateOptions {
    GenerateOptions {
        chart_source: chart_dir.into(),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required,
        emission: SchemaProfile::default().into(),
        authoring: AuthoringPolicy::default(),
        provider: ProviderOptions {
            allow_net: false,
            disable_k8s_schemas: true,
            ..Default::default()
        },
    }
}

fn chart() -> eyre::Result<VfsPath> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: session
            version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: session
            data:
              value: {{ .Values.setting | quote }}
              {{- if .Values.setting }}
              selected: "yes"
              {{- end }}
            {{- if and .Values.firstSwitch .Values.secondSwitch }}
            {{- fail "switches cannot both be enabled" }}
            {{- end }}
        "#},
    )?;
    Ok(chart_dir)
}

fn assert_generated_schema(actual: &GeneratedSchema, expected: &GeneratedSchema) {
    sim_assert_eq!(have: &actual.schema, want: &expected.schema);
    sim_assert_eq!(have: &actual.emission_report, want: &expected.emission_report);
    sim_assert_eq!(have: &actual.definition_origins, want: &expected.definition_origins);
}

#[test]
fn consuming_generation_matches_borrowed_with_cold_and_warm_caches() -> eyre::Result<()> {
    let chart_dir = chart()?;
    for infer_required in [false, true] {
        let options = generate_options(chart_dir.clone(), infer_required);
        let warm = AnalysisSession::new(options.clone());
        let expected = warm.generated_schema()?;
        let cold = AnalysisSession::new(options.clone()).into_generated_schema()?;
        assert_generated_schema(&cold, &expected);
        assert_generated_schema(&warm.into_generated_schema()?, &expected);

        let resolved = AnalysisSession::new(options);
        drop(resolved.resolved_contract()?);
        assert_generated_schema(&resolved.into_generated_schema()?, &expected);

        let validator = jsonschema::validator_for(&cold.schema)?;
        assert!(validator.is_valid(&json!({})));
        assert!(validator.is_valid(&json!({"setting": "selected"})));
    }
    Ok(())
}

fn request(reference_policy: ReferencePolicy) -> EmitRequest {
    EmitRequest {
        reference_policy,
        output: OutputPipelineOptions {
            strip_descriptions: false,
            minimize: true,
            definition_names: DefinitionNames::Source,
        },
    }
}

fn policy_inputs() -> PolicyInputOptions {
    PolicyInputOptions {
        fetch_policy: FetchPolicy::input_assembly(false),
        load_budget: LoadBudget::default(),
    }
}

#[test]
fn consuming_output_matches_borrowed_bytes_for_ordered_overrides() -> eyre::Result<()> {
    let scratch = ScratchDir::new("session-overrides")?;
    let first = scratch.path().join("first-override.json");
    let second = scratch.path().join("second-override.json");
    std::fs::write(
        &first,
        r#"{"properties":{"setting":{"$ref":"./first.json"}}}"#,
    )?;
    std::fs::write(
        &second,
        r#"{"properties":{"setting":{"$ref":"./second.json"}}}"#,
    )?;
    std::fs::write(scratch.path().join("first.json"), r#"{"const":"first"}"#)?;
    std::fs::write(scratch.path().join("second.json"), r#"{"const":"second"}"#)?;
    let paths = [first, second];
    let options = generate_options(chart()?, false);
    let borrowed = AnalysisSession::new(options.clone());
    for reference_policy in [
        ReferencePolicy::SelfContained,
        ReferencePolicy::PreserveRefs,
        ReferencePolicy::FullyInlinedExport,
    ] {
        let request = request(reference_policy);
        let expected = borrowed.emit_with_policy_paths(&paths, policy_inputs(), request)?;
        let result = AnalysisSession::new(options.clone()).into_emit_with_policy_paths(
            &paths,
            policy_inputs(),
            request,
        )?;
        sim_assert_eq!(
            have: serialize_default_schema_json(&result)?.bytes,
            want: serialize_default_schema_json(&expected)?.bytes,
        );
        let restored = if reference_policy == ReferencePolicy::PreserveRefs {
            helm_schema::flatten::flatten_refs(
                &result,
                scratch.path(),
                FetchPolicy::input_assembly(false),
                LoadBudget::default(),
            )?
        } else {
            result
        };
        let validator = jsonschema::validator_for(&restored)?;
        assert!(validator.is_valid(&json!({"setting": "second"})));
        assert!(!validator.is_valid(&json!({"setting": "first"})));
    }
    let request = request(ReferencePolicy::SelfContained);
    let expected = borrowed.emit(request)?;
    let result = AnalysisSession::new(options).into_emit(request)?;
    sim_assert_eq!(
        have: serialize_default_schema_json(&result)?.bytes,
        want: serialize_default_schema_json(&expected)?.bytes,
    );
    Ok(())
}

#[test]
fn consuming_output_preserves_override_error_precedence() -> eyre::Result<()> {
    let scratch = ScratchDir::new("session-bad-override")?;
    let bad = scratch.path().join("invalid-root.json");
    let unresolved = scratch.path().join("unresolved.json");
    std::fs::write(&bad, "[]")?;
    std::fs::write(&unresolved, r#"{"$ref":"./missing.json"}"#)?;
    let empty = generate_options(VfsPath::new(vfs::MemoryFS::new()), false);
    let request = request(ReferencePolicy::SelfContained);
    for error in [
        AnalysisSession::new(empty.clone()).emit_with_policy_paths(
            std::slice::from_ref(&bad),
            policy_inputs(),
            request,
        ),
        AnalysisSession::new(empty.clone()).into_emit_with_policy_paths(
            std::slice::from_ref(&bad),
            policy_inputs(),
            request,
        ),
    ] {
        assert!(
            matches!(error, Err(CliError::InvalidOverrideRoot { path, kind: "array" }) if path == bad)
        );
    }
    let generation_error = AnalysisSession::new(empty.clone())
        .generated_schema()
        .err()
        .ok_or_eyre("empty chart unexpectedly generated a schema")?;
    let result = AnalysisSession::new(empty)
        .into_emit_with_policy_paths(std::slice::from_ref(&unresolved), policy_inputs(), request)
        .err()
        .ok_or_eyre("unresolved override and empty chart unexpectedly emitted a schema")?;
    sim_assert_eq!(have: result.to_string(), want: generation_error.to_string());
    assert!(
        AnalysisSession::new(generate_options(chart()?, false))
            .into_emit_with_policy_paths(&[unresolved], policy_inputs(), request)
            .is_err()
    );
    Ok(())
}
