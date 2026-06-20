mod common;

#[test]
fn ast_corpus_fixtures_match() {
    for case in common::cases::STANDARD_AST_CASES {
        common::assert_ast_fixture(*case);
    }
}
