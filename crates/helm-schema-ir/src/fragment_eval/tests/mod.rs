use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::{DefineIndex, parse_action_expressions, parse_go_template};
use helm_schema_syntax::TemplatedDocument;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;

use super::eval::Interpreter;

/// Assignment mutation passes change the context seen by a helper-valued header.
#[test]
fn assignment_header_results_cross_a_mutation_boundary() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- define "r3a.read" -}}{{ .context.Values.left }}{{- end -}}
    "#};
    let mut defines = DefineIndex::new();
    defines.add_file_source("header.tpl", source);
    let db = IrAnalysisDb::new(&defines);
    let tree = parse_go_template(source).ok_or_eyre("parse header witness")?;
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    let mut interpreter =
        Interpreter::for_source(source, Some("header.tpl"), &db, &tree, &document);
    let copy = parse_action_expressions("{{ $copy := deepCopy . }}");
    interpreter.eval_assignment_exprs(&copy);
    let rhs = r#"include "r3a.read" (dict "context" $copy "mutation" (set $copy.Values "left" .Values.right))"#;
    let expressions = parse_action_expressions(&format!("{{{{ {rhs} }}}}"));
    let before = interpreter.eval_hole_exprs(&expressions);
    let assignment = parse_action_expressions(&format!("{{{{ $value := {rhs} }}}}"));
    interpreter.eval_assignment_exprs(&assignment);
    let after = interpreter.eval_hole_exprs(&expressions);
    let path = |result: crate::eval_effect::EvalResult| {
        result
            .value
            .and_then(|value| value.unique_path())
            .map(|path| path.encode())
    };
    sim_assert_eq!(have: path(before), want: Some("left".to_string()));
    sim_assert_eq!(have: path(after), want: Some("right".to_string()));
    Ok(())
}
