//! Symbolic-IR corpus fixture regressions.

#![recursion_limit = "1024"]

mod common;
use color_eyre::eyre;

#[test]
fn ir_corpus_fixtures_match() -> eyre::Result<()> {
    for case in common::cases::STANDARD_IR_CASES {
        common::assert_ir_fixture(case)?;
    }
    Ok(())
}
