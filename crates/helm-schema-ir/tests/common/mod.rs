#![allow(
    dead_code,
    reason = "shared by multiple integration-test binaries; each binary uses a subset"
)]

use helm_schema_ast::DefineIndex;
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

pub fn build_define_index(spec: test_util::DefineSourceSpec<'_>) -> DefineIndex {
    let loaded = spec.load();
    let mut idx = DefineIndex::new();
    for (idx_num, source) in loaded.helper_templates.into_iter().enumerate() {
        idx.add_file_source(&format!("<inline:{idx_num}>"), &source);
    }
    for (name, source) in loaded.file_sources {
        idx.add_file_source(&name, &source);
    }
    idx
}

pub fn render_ir_case(case: IrCorpusCase<'_>) -> Value {
    let src = test_util::read_testdata(case.template_path);
    let idx = build_define_index(case.define_sources);
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(&src)
        .finalize()
        .document();

    let actual = serde_json::to_value(ir).expect("serialize");
    if std::env::var(case.dump_env).is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
        let dump_stem = case
            .template_path
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        let path = std::env::temp_dir().join(format!("helm-schema-ir.{dump_stem}.ir.json"));
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&actual).expect("json bytes"),
        )
        .expect("write ir dump");
    }
    actual
}

pub fn assert_ir_fixture(case: IrCorpusCase<'_>) {
    let actual = render_ir_case(case);
    if std::env::var(case.dump_env).is_ok() {
        return;
    }
    let expected: Value = serde_json::from_str(case.expected_fixture).expect("expected ir json");

    sim_assert_eq!(have: actual, want: expected);
}
