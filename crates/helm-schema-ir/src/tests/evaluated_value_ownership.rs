//! Ownership contracts observed at frozen expression output boundaries.

use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, Predicate, PredicateMemo, ValuesPath};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;

/// An opaque primary retains its input influence and its literal fallback.
#[test]
fn default_retains_an_explicit_unknown_primary() -> eyre::Result<()> {
    let source = r#"{{ printf "%s" .Values.alpha | default "fallback" }}"#;
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    let execution = crate::value::execute_document(source, None, &db);
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing default output")?;
    sim_assert_eq!(have: site.value().inputs(), want: BTreeSet::from([ValuesPath::parse("alpha")]));
    sim_assert_eq!(have: site.projected_value().strings(), want: BTreeSet::from(["fallback".to_string()]));
    sim_assert_eq!(have: site.value().literal(), want: None);
    sim_assert_eq!(have: site.projected_scalar(), want: None);
    Ok(())
}

/// `default` selects literal null and truthy non-string primaries by Helm truthiness.
#[test]
fn default_type_truth_handles_null_and_truthy_nonstring_primaries() -> eyre::Result<()> {
    for (source, expected) in [
        (r#"typeIs "string" (default "fallback" nil)"#, true),
        (r#"typeIs "bool" (default "fallback" true)"#, true),
        (r#"typeIs "string" (default "fallback" true)"#, false),
    ] {
        let defines = DefineIndex::new();
        let db = IrAnalysisDb::new(&defines);
        let source = format!("{{{{ {source} }}}}");
        let execution = crate::value::execute_document(&source, None, &db);
        let (_, output) = execution.text().next().ok_or_eyre("missing type output")?;
        sim_assert_eq!(have: output.exact_text(), want: Some(expected.to_string()), "source={source}");
    }
    Ok(())
}

/// Type tests follow the runtime arm selected by an ordered `default` value.
#[test]
fn default_type_truth_tracks_the_selected_dynamic_value() -> eyre::Result<()> {
    let source =
        r#"{{ if typeIs "string" (default "fallback" .Values.primary) }}selected{{ end }}"#;
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    let execution = crate::value::execute_document(source, None, &db);
    let predicate = PredicateMemo::new().normalize(Predicate::Or(vec![
        Predicate::truthy_path("primary").negated(),
        Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("primary"),
            schema_type: "string".to_string(),
        }),
    ]));
    let selected = execution
        .text()
        .filter_map(|(condition, text)| {
            (text.exact_text().as_deref() == Some("selected")).then_some(condition)
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(have: Predicate::Or(selected).normalize_boolean(), want: predicate);
    Ok(())
}
