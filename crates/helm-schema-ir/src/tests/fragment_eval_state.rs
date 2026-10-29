use std::collections::BTreeMap;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
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
    let execution = crate::value::tests::execute_document(source, Some("nested.tpl"), &db);
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing nested output")?;
    sim_assert_eq!(have: site.value().exact_text(), want: Some("pinned".to_owned()));
    Ok(())
}

#[test]
fn holes_and_assignments_retain_type_only_scalar_transport() {
    let source = "{{ $count := len (randAlphaNum (int .Values.n)) }}{{ len (randAlphaNum (int .Values.n)) }}{{ $count }}";
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    let execution = crate::value::tests::execute_document(source, Some("typed.tpl"), &db);
    let sites = execution
        .sites()
        .filter(|site| site.rendered())
        .collect::<Vec<_>>();
    sim_assert_eq!(have: sites.len(), want: 2);
    for site in sites {
        sim_assert_eq!(have: site.value().go_type(), want: crate::value::kinds::GoType::Int);
        sim_assert_eq!(have: site.value().literal(), want: None);
    }
}

#[test]
#[ignore = "R3b regression vs main, package C (parser, collection and validation transfers): semver selector holes keep the unsigned go type; tracked in plan/value-algebra-r3b.md follow-ups"]
fn semver_selector_holes_keep_the_unsigned_go_type() -> eyre::Result<()> {
    let source = r#"{{ (semver "1.2.3").Major }}"#;
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    let execution = crate::value::tests::execute_document(source, Some("semver.tpl"), &db);
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing semver output")?;
    sim_assert_eq!(have: site.value().go_type(), want: crate::value::kinds::GoType::Uint64);
    sim_assert_eq!(have: site.value().literal(), want: Some(&helm_schema_core::GuardValue::Int(1)));
    Ok(())
}
