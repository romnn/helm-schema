//! Symbolic-IR corpus fixture regressions.

#![recursion_limit = "1024"]

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema_test_support::generate;
use helm_schema_test_support::registry::{ArtifactId, ArtifactTarget, IrId};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

#[test]
fn ir_corpus_fixtures_match() -> eyre::Result<()> {
    for id in IrId::ALL {
        let actual = generate::ir_document(&id.case().recipe)?;
        let spec = ArtifactId::Ir(*id).spec();
        let ArtifactTarget::Fixture(fixture) = &spec.target else {
            eyre::bail!("{id:?} is registered without a fixture");
        };
        let fixture_path = test_util::workspace_root().join(fixture);
        let expected: Value = serde_json::from_str(
            &std::fs::read_to_string(&fixture_path)
                .wrap_err_with(|| format!("read {}", fixture_path.display()))?,
        )
        .wrap_err("parse expected contract IR fixture")?;

        sim_assert_eq!(have: actual, want: expected, "{} IR fixture mismatch", spec.dump_name);
    }
    Ok(())
}
