use helm_schema_ast::{HelmParser, TreeSitterParser};
use test_util::prelude::sim_assert_eq;

pub mod cases;

#[derive(Clone, Copy)]
pub struct AstCorpusCase<'a> {
    pub template_path: &'a str,
    pub expected_fixture: &'a str,
}

pub fn assert_ast_fixture(case: AstCorpusCase<'_>) {
    let src = test_util::read_testdata(case.template_path);
    let ast = TreeSitterParser.parse(&src).expect("parse");
    sim_assert_eq!(have: ast.to_sexpr(), want: case.expected_fixture.trim_end());
}
