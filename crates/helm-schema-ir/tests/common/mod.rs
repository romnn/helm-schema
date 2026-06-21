use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::SymbolicIrContext;
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

pub mod cases;

#[derive(Clone, Copy)]
pub struct IrCorpusCase<'a> {
    pub template_path: &'a str,
    pub expected_fixture: &'a str,
    pub define_sources: test_util::DefineSourceSpec<'a>,
    pub dump_env: &'a str,
}

pub fn build_define_index(
    parser: &dyn HelmParser,
    spec: test_util::DefineSourceSpec<'_>,
) -> DefineIndex {
    let loaded = spec.load();
    let mut idx = DefineIndex::new();
    for source in loaded.helper_templates {
        idx.add_source(parser, &source)
            .expect("helper source should parse");
    }
    for (name, source) in loaded.file_sources {
        idx.add_file_source(&name, &source);
    }
    idx
}

pub fn render_ir_case(case: IrCorpusCase<'_>) -> Value {
    let src = test_util::read_testdata(case.template_path);
    let idx = build_define_index(&TreeSitterParser, case.define_sources);
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(&src, &idx)
        .document();

    let actual = serde_json::to_value(ir).expect("serialize");
    if std::env::var(case.dump_env).is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }
    actual
}

pub fn assert_ir_fixture(case: IrCorpusCase<'_>) {
    let actual = render_ir_case(case);
    let expected: Value = serde_json::from_str(case.expected_fixture).expect("expected ir json");

    sim_assert_eq!(have: actual, want: expected);
}
