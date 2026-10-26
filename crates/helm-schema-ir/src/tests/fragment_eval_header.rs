use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;

/// Helper entry sees mutations performed while evaluating its context arguments.
#[test]
fn assignment_header_results_cross_a_mutation_boundary() -> eyre::Result<()> {
    let helper = indoc! {r#"
        {{- define "r3a.read" -}}{{ .context.Values.left }}{{- end -}}
    "#};
    let mut defines = DefineIndex::new();
    defines.add_file_source("_helpers.tpl", helper);
    let db = IrAnalysisDb::new(&defines);
    let source = indoc! {r#"
        {{- $_ := set .Values "left" "left" -}}
        {{- $_ := set .Values "right" "right" -}}
        {{- $copy := deepCopy . -}}
        {{- include "r3a.read" (dict "context" $copy "mutation" (set $copy.Values "left" .Values.right)) -}}
        {{- $value := include "r3a.read" (dict "context" $copy "mutation" (set $copy.Values "left" .Values.right)) -}}
        {{- include "r3a.read" (dict "context" $copy "mutation" (set $copy.Values "left" .Values.right)) -}}
    "#};
    let execution = crate::value::execute_document(source, Some("header.tpl"), &db);
    let output = execution
        .text()
        .next()
        .ok_or_eyre("missing completed execution")?
        .1;
    sim_assert_eq!(have: output.exact_text(), want: Some("rightright".to_string()));
    Ok(())
}

#[path = "fragment_eval_state.rs"]
mod state;
