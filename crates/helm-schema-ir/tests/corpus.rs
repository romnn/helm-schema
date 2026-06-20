#![recursion_limit = "1024"]

mod common;

#[test]
fn ir_corpus_fixtures_match() {
    for case in common::cases::STANDARD_IR_CASES {
        common::assert_ir_fixture(*case);
    }
}
