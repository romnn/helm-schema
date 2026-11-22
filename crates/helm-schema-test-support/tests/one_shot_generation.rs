//! Verifies corpus recipes preserve bytes when they consume their analysis sessions.

use color_eyre::eyre;
use helm_schema::AnalysisSession;
use helm_schema_test_support::generate::{
    emit_policy, generate, generate_options, policy_session, session_schema,
};
use helm_schema_test_support::registry::{GenerationRecipe, PolicyId};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

fn fixture_bytes(schema: &Value) -> eyre::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(schema)?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[test]
fn one_shot_recipes_match_borrowed_artifact_bytes() -> eyre::Result<()> {
    let chart = PolicyId::Full.recipe().chart;
    let borrowed = AnalysisSession::new(generate_options(&chart));
    sim_assert_eq!(
        have: generate(&GenerationRecipe::Chart(chart))?,
        want: fixture_bytes(&session_schema(&borrowed, &chart)?)?,
    );
    for policy in [PolicyId::Full, PolicyId::CallerOverwrite] {
        let policy = policy.recipe();
        let borrowed = policy_session(&policy);
        sim_assert_eq!(
            have: generate(&GenerationRecipe::FinalPolicy(policy))?,
            want: fixture_bytes(&emit_policy(&borrowed, &policy)?)?,
        );
    }
    Ok(())
}
