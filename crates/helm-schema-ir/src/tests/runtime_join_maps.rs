use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{GuardValue, Predicate};
use indoc::formatdoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;
use crate::value::{Coverage, Field, Remainder, State, Value};

fn survivor_map_rejects_a(consumer: &str) {
    let source = formatdoc! {r#"
        {{{{- $d := dict -}}}}
        {{{{- if .Values.a -}}}}
          {{{{- if .Values.b -}}}}{{{{- $_ := set $d "k" 1 -}}}}
          {{{{- else -}}}}{{{{- fail "inner" -}}}}{{{{- end -}}}}
        {{{{- end -}}}}
        {{{{- if {consumer} -}}}}{{{{- fail "outer" -}}}}{{{{- end -}}}}
    "#};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(&source, None, &db);
    let aborted = execution.abort_predicate();
    let expected = Predicate::truthy_path("a");
    assert!(
        expected.exactly_implies(&aborted),
        "{consumer}: lost rejection: {aborted:?}"
    );
    assert!(
        aborted.exactly_implies(&expected),
        "{consumer}: fabricated rejection: {aborted:?}"
    );
}

#[test]
fn frontier_map_truthiness_preserves_presence_correlation() {
    survivor_map_rejects_a("$d");
}

#[test]
fn frontier_map_length_preserves_presence_correlation() {
    survivor_map_rejects_a("len $d");
}

#[test]
fn frontier_map_has_key_preserves_presence_correlation() {
    survivor_map_rejects_a("hasKey $d \"k\"");
}

#[test]
fn frontier_map_empty_preserves_presence_correlation() {
    survivor_map_rejects_a("not (empty $d)");
}

#[test]
fn frontier_map_keys_preserves_presence_correlation() {
    survivor_map_rejects_a("len (keys $d)");
}

#[test]
fn restricted_frontier_map_truthiness_sees_conditional_presence() -> eyre::Result<()> {
    let mut initial = State::new(Value::nil());
    let map = initial.heap.map(Remainder::Closed);
    let target = initial.map_target(&map).ok_or_eyre("missing map target")?;
    let a = initial.heap.arena.decision(
        Predicate::truthy_path("a"),
        Predicate::truthy_path("a").negated(),
        Coverage::Exact,
    );
    let b = initial.heap.arena.decision(
        Predicate::truthy_path("b"),
        Predicate::truthy_path("b").negated(),
        Coverage::Exact,
    );
    let mut yes = initial.fork(a.clone(), true).fork(b.clone(), true);
    yes.heap.write(
        &target,
        "k",
        Field::Present(Value::literal(GuardValue::Int(1))),
    );
    let no = initial.fork(a.clone(), false);
    let joined = State::join_branches(
        &initial.heap.arena.join_paths(&yes.active, &no.active),
        &yes,
        &no,
    )
    .ok_or_eyre("join declined")?;
    sim_assert_eq!(have: joined.fork(a.clone(), true).fork(b, true).truth(&map), want: Some(true));
    sim_assert_eq!(have: joined.fork(a, false).truth(&map), want: Some(false));
    Ok(())
}
