//! Reading produced artifacts, or generating one when no producer ran.

use std::path::Path;

use color_eyre::eyre::{self, WrapErr as _};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

use crate::generate;
use crate::manifest::{self, InputDigester};
use crate::registry::{ArtifactId, ArtifactSpec, ArtifactTarget};

/// Names the producer output directory a test run should read.
///
/// Unset means no producer ran: every consumer generates its own artifact from
/// the current checkout and compares it with its committed fixture. Set means
/// the directory's manifest is the evidence, and any mismatch with the build,
/// the checkout or the registry fails the consumer.
pub const ARTIFACTS_ENV: &str = "HELM_SCHEMA_CORPUS_ARTIFACTS";

/// The artifact `id`, from the producer named by [`ARTIFACTS_ENV`] or generated locally.
///
/// # Errors
///
/// Returns an error when local generation fails, or when a supplied manifest
/// is missing, incomplete, stale, or disagrees with the artifact bytes.
///
/// # Panics
///
/// Panics with a diff when a locally generated artifact differs from its
/// committed fixture.
pub fn consume(id: ArtifactId) -> eyre::Result<Value> {
    let artifacts = std::env::var_os(ARTIFACTS_ENV);
    consume_from(artifacts.as_deref().map(Path::new), &id.spec())
}

/// The artifact of `spec`, read from the producer output in `artifacts` or,
/// when that is `None`, generated from the current checkout.
///
/// A produced artifact is returned only after [`manifest::load`] accepted the
/// complete manifest and [`manifest::read_artifact`] rechecked the recipe's
/// inputs and the artifact's bytes. A locally generated artifact with a
/// committed fixture must equal that fixture, so a filtered test still proves
/// full fixture equality; internal artifacts have no fixture to compare.
///
/// # Errors
///
/// Returns an error when local generation fails or any provenance check fails.
///
/// # Panics
///
/// Panics with a diff when a locally generated artifact differs from its
/// committed fixture.
pub fn consume_from(artifacts: Option<&Path>, spec: &ArtifactSpec) -> eyre::Result<Value> {
    let key = spec.id.key();
    let Some(artifacts) = artifacts else {
        let generated: Value = serde_json::from_slice(&generate::generate(&spec.recipe)?)
            .wrap_err_with(|| format!("parse generated {key}"))?;
        if let ArtifactTarget::Fixture(fixture) = &spec.target {
            let path = test_util::workspace_root().join(fixture);
            let committed: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .wrap_err_with(|| format!("read fixture {}", path.display()))?,
            )
            .wrap_err_with(|| format!("parse fixture {}", path.display()))?;
            sim_assert_eq!(
                have: generated,
                want: committed,
                "{key}: the locally generated artifact differs from its fixture {fixture}"
            );
        }
        return Ok(generated);
    };
    let manifest = manifest::load(artifacts)?;
    let mut inputs = InputDigester::new(test_util::workspace_testdata());
    let bytes = manifest::read_artifact(artifacts, &manifest, spec, &mut inputs)?;
    serde_json::from_slice(&bytes).wrap_err_with(|| format!("parse artifact {key}"))
}
