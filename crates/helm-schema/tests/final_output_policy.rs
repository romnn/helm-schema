//! Final-output policy annotation fixtures.

use std::path::Path;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema::generation::{GenerateOptions, SchemaProfile};
use helm_schema::output::{
    FetchPolicy, LoadBudget, OutputPipelineOptions, PolicyInputOptions, PolicyInputs, ReferenceMode,
};
use helm_schema::provider::ProviderOptions;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

const CHART: &str = "schema-emission-controls";

#[test]
fn final_outputs_match_policy_annotation_fixtures() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let full_session = profile_session(SchemaProfile::Full, false);
    let lean_session = profile_session(SchemaProfile::Lean, false);
    let full = full_session.emit(
        PolicyInputs::default(),
        output_options(ReferenceMode::SelfContained),
    )?;
    let lean = lean_session.emit(
        PolicyInputs::default(),
        output_options(ReferenceMode::SelfContained),
    )?;

    assert_fixture("full", &full)?;
    assert_fixture("lean", &lean)?;
    sim_assert_eq!(
        have: full_session.generated_schema()?.schema.get("x-helm-schema-policy"),
        want: None
    );

    let repeated = lean_session.emit(
        PolicyInputs::default(),
        output_options(ReferenceMode::SelfContained),
    )?;
    sim_assert_eq!(have: repeated, want: lean);
    Ok(())
}

#[test]
fn overrides_cannot_forge_policy_annotations_or_boolean_root_identity() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let session = profile_session(SchemaProfile::Full, false);
    let tempdir = tempfile::tempdir().wrap_err("create final-output fixture directory")?;

    let caller_path = tempdir.path().join("caller.json");
    std::fs::write(
        &caller_path,
        serde_json::to_vec(&json!({
            "x-helm-schema-generated": false,
            "x-helm-schema-policy": { "forged": true },
            "description": "caller override"
        }))?,
    )
    .wrap_err("write caller override")?;
    let caller = emit_with_override(&session, &caller_path, ReferenceMode::PreserveRefs)?;
    assert_fixture("caller-overwrite", &caller)?;
    sim_assert_eq!(
        have: caller["x-helm-schema-policy"]["requested-profile"].clone(),
        want: json!("full")
    );

    let boolean_path = tempdir.path().join("boolean.json");
    std::fs::write(&boolean_path, b"false\n").wrap_err("write Boolean override")?;
    let boolean = emit_with_override(&session, &boolean_path, ReferenceMode::SelfContained)?;
    assert_fixture("boolean-false", &boolean)?;
    let validator = jsonschema::validator_for(&boolean)?;
    sim_assert_eq!(have: validator.is_valid(&json!({})), want: false);
    Ok(())
}

#[test]
fn narrowing_and_reference_modifiers_change_the_policy_fingerprint() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let ordinary = profile_session(SchemaProfile::Full, false).emit(
        PolicyInputs::default(),
        output_options(ReferenceMode::SelfContained),
    )?;
    let narrowed = profile_session(SchemaProfile::Full, true).emit(
        PolicyInputs::default(),
        output_options(ReferenceMode::SelfContained),
    )?;
    let preserved = profile_session(SchemaProfile::Full, false).emit(
        PolicyInputs::default(),
        output_options(ReferenceMode::PreserveRefs),
    )?;

    sim_assert_eq!(
        have: narrowed["x-helm-schema-policy"]["narrowing"].clone(),
        want: json!(["infer-required"])
    );
    let ordinary_fingerprint = fingerprint(&ordinary)?;
    eyre::ensure!(ordinary_fingerprint != fingerprint(&narrowed)?);
    eyre::ensure!(ordinary_fingerprint != fingerprint(&preserved)?);
    Ok(())
}

fn emit_with_override(
    session: &helm_schema::AnalysisSession,
    path: &Path,
    reference_mode: ReferenceMode,
) -> eyre::Result<Value> {
    Ok(session.emit_with_policy_paths(
        &[path.to_path_buf()],
        PolicyInputOptions {
            reference_mode,
            fetch_policy: FetchPolicy::input_assembly(false),
            load_budget: LoadBudget::default(),
        },
        output_options(reference_mode),
    )?)
}

fn output_options(reference_mode: ReferenceMode) -> OutputPipelineOptions {
    OutputPipelineOptions {
        reference_mode,
        strip_descriptions: false,
        minimize: true,
    }
}

fn profile_session(profile: SchemaProfile, infer_required: bool) -> helm_schema::AnalysisSession {
    let chart_dir = test_util::workspace_testdata().join("charts").join(CHART);
    let chart_dir = chart_dir.to_string_lossy().to_string();
    helm_schema::AnalysisSession::new(GenerateOptions {
        chart_dir: VfsPath::new(vfs::PhysicalFS::new(&chart_dir)),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required,
        profile,
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
    })
}

fn assert_fixture(name: &str, actual: &Value) -> eyre::Result<()> {
    let fixture_path = test_util::workspace_testdata()
        .join("final-output-schemas")
        .join(format!("{name}.schema.json"));
    if std::env::var("SCHEMA_DUMP").is_ok() {
        let dump_path =
            std::env::temp_dir().join(format!("helm-schema.final-output.{name}.schema.json"));
        let mut bytes = serde_json::to_vec_pretty(actual).wrap_err("serialize final output")?;
        bytes.push(b'\n');
        std::fs::write(&dump_path, bytes)
            .wrap_err_with(|| format!("write {}", dump_path.display()))?;
    }
    if !fixture_path.exists() && std::env::var("SCHEMA_DUMP").is_ok() {
        return Ok(());
    }
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(&fixture_path)
            .wrap_err_with(|| format!("read {}", fixture_path.display()))?,
    )
    .wrap_err_with(|| format!("parse {}", fixture_path.display()))?;
    sim_assert_eq!(have: actual, want: &expected, "{name}: final output fixture mismatch");
    Ok(())
}

fn fingerprint(schema: &Value) -> eyre::Result<&str> {
    schema
        .pointer("/x-helm-schema-policy/policy-fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre::eyre!("schema has no policy fingerprint"))
}
