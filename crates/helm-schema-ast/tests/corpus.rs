//! Corpus regressions for typed Helm-template expression extraction.

mod common;
use color_eyre::eyre;

#[test]
fn ast_corpus_fixtures_match() -> eyre::Result<()> {
    for case in common::cases::STANDARD_AST_CASES {
        common::assert_ast_fixture(*case)?;
    }
    Ok(())
}
