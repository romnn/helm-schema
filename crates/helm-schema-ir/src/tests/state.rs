use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::Predicate;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;

fn outputs(source: &str) -> eyre::Result<BTreeSet<(Predicate, String)>> {
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    crate::value::tests::execute_document(source, None, &db)
        .text()
        .map(|(condition, text)| Ok((condition, text.exact_text().ok_or_eyre("unknown output")?)))
        .collect()
}

#[test]
fn local_and_root_restores_preserve_the_other_namespace() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- $local := "before" -}}
        {{- $_ := set . "field" "before" -}}
        {{- with true -}}
          {{- $local := "after" -}}
          {{- $_ := set $ "field" "after" -}}
        {{- end -}}
        {{- print $local "/" .field -}}
    "#};
    sim_assert_eq!(have: outputs(source)?, want: BTreeSet::from([(Predicate::True, "before/after".to_string())]));
    Ok(())
}

#[test]
fn local_join_and_scope_exit_preserve_root_writes() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- $local := "before" -}}
        {{- if .Values.enabled -}}
          {{- $local := "shadow" -}}
          {{- $_ := set . "field" "after" -}}
        {{- else -}}
          {{- $_ := set . "field" "after" -}}
        {{- end -}}
        {{- print $local "/" .field -}}
    "#};
    sim_assert_eq!(have: outputs(source)?, want: BTreeSet::from([(Predicate::True, "before/after".to_string())]));
    Ok(())
}

#[test]
fn root_join_retains_exact_scalar_arms() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- if .Values.enabled -}}
          {{- $_ := set . "field" "left" -}}
        {{- else -}}
          {{- $_ := set . "field" "right" -}}
        {{- end -}}
        {{- .field -}}
    "#};
    let condition = Predicate::truthy_path("enabled");
    sim_assert_eq!(have: outputs(source)?, want: BTreeSet::from([
        (condition.clone(), "left".to_string()),
        (condition.negated(), "right".to_string()),
    ]));
    Ok(())
}
