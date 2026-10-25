use std::collections::BTreeMap;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::{DefineIndex, parse_action_expressions, parse_go_template};
use helm_schema_syntax::TemplatedDocument;
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::fragment_eval::eval::Interpreter;
use crate::symbolic::SymbolicPolicy;

#[test]
fn nested_template_preserves_static_root_fields() -> eyre::Result<()> {
    let source = r#"{{ tpl "{{ .Release.Name }}" . }}"#;
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::with_policy(
        &defines,
        SymbolicPolicy {
            static_root_strings: BTreeMap::from([(
                vec!["Release".to_owned(), "Name".to_owned()],
                "pinned".to_owned(),
            )]),
            ..SymbolicPolicy::default()
        },
    );
    let tree = parse_go_template(source).ok_or_eyre("parse nested template witness")?;
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    let mut interpreter =
        Interpreter::for_source(source, Some("nested.tpl"), &db, &tree, &document);
    let expressions = parse_action_expressions(source);
    sim_assert_eq!(
        have: interpreter.inline_static_template_value(&expressions),
        want: Some(AbstractValue::StringSet(["pinned".to_owned()].into()))
    );
    Ok(())
}

#[test]
fn holes_and_assignments_retain_type_only_scalar_transport() -> eyre::Result<()> {
    let source = "{{ $count := len (opaque) }}";
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    let tree = parse_go_template(source).ok_or_eyre("parse typed assignment")?;
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    let mut interpreter = Interpreter::for_source(source, Some("typed.tpl"), &db, &tree, &document);
    let expected =
        crate::scalar_value::ScalarValueDispatch::typed(crate::value::kinds::GoType::Int);
    let direct = interpreter.eval_hole_exprs(&parse_action_expressions("{{ len (opaque) }}"));
    sim_assert_eq!(have: direct.scalar_transport(), want: Some(&expected));
    interpreter.eval_assignment_exprs(&parse_action_expressions(source));
    let assigned = interpreter.eval_hole_exprs(&parse_action_expressions("{{ $count }}"));
    sim_assert_eq!(have: assigned.scalar_transport(), want: Some(&expected));
    Ok(())
}

#[test]
fn semver_selector_holes_keep_the_unsigned_go_type() -> eyre::Result<()> {
    let source = r#"{{ (semver "1.2.3").Major }}"#;
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    let tree = parse_go_template(source).ok_or_eyre("parse typed semver selector")?;
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    let mut interpreter =
        Interpreter::for_source(source, Some("semver.tpl"), &db, &tree, &document);
    let result = interpreter.eval_hole_exprs(&parse_action_expressions(source));
    let mut expected =
        crate::scalar_value::ScalarValueDispatch::constant(helm_schema_core::GuardValue::Int(1));
    expected.refine_type(crate::value::kinds::GoType::Uint64, db.predicate_memo());
    sim_assert_eq!(have: result.scalar_transport(), want: Some(&expected));
    Ok(())
}
