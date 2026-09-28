//! Final-output policy annotation fixtures.

use std::path::Path;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema::output::{
    EmitRequest, FetchPolicy, LoadBudget, OutputPipelineOptions, PolicyInputOptions,
    ReferencePolicy,
};
use helm_schema_test_support::generate;
use helm_schema_test_support::registry::{ArtifactId, ArtifactTarget, PolicyId};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

#[test]
fn final_outputs_match_policy_annotation_fixtures() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let full_recipe = PolicyId::Full.recipe();
    let lean_recipe = PolicyId::Lean.recipe();
    let full_session = generate::policy_session(&full_recipe);
    let lean_session = generate::policy_session(&lean_recipe);
    let full = generate::emit_policy(&full_session, &full_recipe)?;
    let lean = generate::emit_policy(&lean_session, &lean_recipe)?;

    assert_fixture(PolicyId::Full, &full)?;
    assert_fixture(PolicyId::Lean, &lean)?;
    sim_assert_eq!(
        have: full_session.generated_schema()?.schema.get("x-helm-schema-policy"),
        want: None
    );

    let repeated = generate::emit_policy(&lean_session, &lean_recipe)?;
    sim_assert_eq!(have: repeated, want: lean);
    Ok(())
}

#[test]
fn overrides_cannot_forge_policy_annotations_or_boolean_root_identity() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let caller_recipe = PolicyId::CallerOverwrite.recipe();
    let boolean_recipe = PolicyId::BooleanFalse.recipe();
    // One session serves both overrides: override loading is an output-stage
    // concern and must not depend on a fresh analysis.
    sim_assert_eq!(have: boolean_recipe.chart, want: caller_recipe.chart);
    let session = generate::policy_session(&caller_recipe);

    let caller = generate::emit_policy(&session, &caller_recipe)?;
    assert_fixture(PolicyId::CallerOverwrite, &caller)?;
    sim_assert_eq!(
        have: caller["x-helm-schema-policy"]["requested-profile"].clone(),
        want: json!("full")
    );

    let boolean = generate::emit_policy(&session, &boolean_recipe)?;
    assert_fixture(PolicyId::BooleanFalse, &boolean)?;
    let validator = jsonschema::validator_for(&boolean)?;
    sim_assert_eq!(have: validator.is_valid(&json!({})), want: false);
    Ok(())
}

#[test]
fn narrowing_and_reference_modifiers_change_the_policy_fingerprint() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let ordinary_recipe = PolicyId::Full.recipe();
    let mut narrowed_recipe = ordinary_recipe;
    narrowed_recipe.chart.infer_required = true;
    let mut preserved_recipe = ordinary_recipe;
    preserved_recipe.reference_policy = ReferencePolicy::PreserveRefs;
    let ordinary = generate::emit_policy(
        &generate::policy_session(&ordinary_recipe),
        &ordinary_recipe,
    )?;
    let narrowed = generate::emit_policy(
        &generate::policy_session(&narrowed_recipe),
        &narrowed_recipe,
    )?;
    let preserved = generate::emit_policy(
        &generate::policy_session(&preserved_recipe),
        &preserved_recipe,
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

#[test]
fn override_loading_and_root_validation_precede_chart_generation() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let tempdir = ScratchDir::new("final_output_policy")
        .wrap_err("create invalid-input fixture directory")?;
    let override_path = tempdir.path().join("invalid-override.json");
    std::fs::write(&override_path, b"null\n").wrap_err("write invalid override")?;
    let missing_chart = tempdir.path().join("missing-chart");
    let session = helm_schema::AnalysisSession::new(generate::generate_options_at(
        &missing_chart,
        &PolicyId::Full.recipe().chart,
    ));

    let error = emit_with_override(&session, &override_path, ReferencePolicy::PreserveRefs)
        .expect_err("invalid override root should fail before chart generation");

    sim_assert_eq!(
        have: error.to_string(),
        want: format!(
            "override schema root in {} must be an object or boolean, found null",
            override_path.display()
        )
    );

    let missing_override = tempdir.path().join("missing-override.json");
    let result = session.emit_with_policy_paths(
        &[missing_override],
        PolicyInputOptions {
            fetch_policy: FetchPolicy::input_assembly(false),
            load_budget: LoadBudget::default(),
        },
        emit_request(ReferencePolicy::PreserveRefs),
    );
    sim_assert_eq!(
        have: matches!(result, Err(helm_schema::CliError::Io(_))),
        want: true
    );
    Ok(())
}

fn emit_with_override(
    session: &helm_schema::AnalysisSession,
    path: &Path,
    reference_policy: ReferencePolicy,
) -> eyre::Result<Value> {
    Ok(session.emit_with_policy_paths(
        &[path.to_path_buf()],
        PolicyInputOptions {
            fetch_policy: FetchPolicy::input_assembly(false),
            load_budget: LoadBudget::default(),
        },
        emit_request(reference_policy),
    )?)
}

fn emit_request(reference_policy: ReferencePolicy) -> EmitRequest {
    EmitRequest {
        reference_policy,
        output: OutputPipelineOptions {
            strip_descriptions: false,
            minimize: true,
            definition_names: helm_schema::output::DefinitionNames::Source,
        },
    }
}

fn assert_fixture(id: PolicyId, actual: &Value) -> eyre::Result<()> {
    let spec = ArtifactId::FinalPolicy(id).spec();
    let ArtifactTarget::Fixture(fixture) = &spec.target else {
        eyre::bail!("{id:?} is registered without a fixture");
    };
    let fixture_path = test_util::workspace_root().join(fixture);
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(&fixture_path)
            .wrap_err_with(|| format!("read {}", fixture_path.display()))?,
    )
    .wrap_err_with(|| format!("parse {}", fixture_path.display()))?;
    sim_assert_eq!(
        have: actual,
        want: &expected,
        "{}: final output fixture mismatch",
        id.name()
    );
    Ok(())
}

fn fingerprint(schema: &Value) -> eyre::Result<&str> {
    schema
        .pointer("/x-helm-schema-policy/policy-fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre::eyre!("schema has no policy fingerprint"))
}
