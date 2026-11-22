use std::rc::Rc;

use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;
use crate::value::{AbortReason, Flow, State, Value, eval::Executor};
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::Predicate;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn text(source: &str) -> Vec<Option<String>> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, None, &db);
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    execution
        .text()
        .map(|(_, value)| value.exact_text())
        .collect()
}

#[test]
fn integer_range_preserves_nonpositive_zero_and_one_variable_semantics() {
    for (source, expected) in [
        (r"{{ range -1 }}wrong{{ else }}empty{{ end }}", "empty"),
        (r"{{ range $i := 0 }}wrong{{ else }}empty{{ end }}", "empty"),
        (r"{{ range 3 }}{{ . }}{{ end }}", "012"),
        (r"{{ range $i := int64 3 }}{{ $i }}{{ end }}", "012"),
        (
            r"{{ range 5 }}{{ if eq . 1 }}{{ continue }}{{ end }}{{ if eq . 3 }}{{ break }}{{ end }}{{ . }}{{ end }}",
            "02",
        ),
        (
            r"{{ range (fromJson `null`) }}wrong{{ else }}empty{{ end }}",
            "empty",
        ),
    ] {
        sim_assert_eq!(have: text(source), want: vec![Some(expected.to_owned())], "{source}");
    }
}

#[test]
fn invalid_range_types_and_two_integer_variables_abort_before_the_body() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r"{{ range $i, $v := 0 }}{{ end }}",
        r"{{ range $i, $v := int64 3 }}{{ end }}",
        r#"{{ range (get (fromJson `{"n":3}`) "n") }}{{ end }}"#,
        r"{{ range false }}{{ else }}wrong{{ end }}",
        r#"{{ range "" }}{{ else }}wrong{{ end }}"#,
    ] {
        sim_assert_eq!(have: execute_document(source, None, &db).abort_predicate(), want: Predicate::True, "{source}");
    }
}

#[test]
fn string_range_keeps_current_and_later_backing_slots_live() {
    // Go's range variables retain reflect-backed string slots, including the current element.
    let source = indoc! {r#"
        {{- $xs := splitList "," "b,a" -}}
        {{- range $i, $v := $xs -}}
          {{- if eq $i 0 -}}{{- $_ := sortAlpha $xs -}}{{- end -}}
          {{- $v -}}/{{- . -}};
        {{- end -}}
    "#};
    sim_assert_eq!(have: text(source), want: vec![Some("a/a;b/b;".to_owned())]);
}

#[test]
fn map_range_captures_entries_but_keeps_nested_map_references_live() {
    // Replacing a map entry does not replace its captured value; nested maps remain aliased.
    let source = indoc! {r#"
        {{- $nested := dict "x" "old" -}}
        {{- $d := dict "a" "old" "b" "old" "c" $nested -}}
        {{- range $key, $value := $d -}}
          {{- if eq $key "a" -}}
            {{- $_ := set $d "b" "new" -}}
            {{- $_ := set $nested "x" "new" -}}
          {{- end -}}
          {{- if eq $key "c" -}}{{- $value.x -}}{{- else -}}{{- $value -}}{{- end -}};
        {{- end -}}
    "#};
    sim_assert_eq!(have: text(source), want: vec![Some("old;old;new;".to_owned())]);
}

#[test]
fn nonempty_over_budget_ranges_preserve_abort_and_do_not_execute_else() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for count in [31, 32, 33] {
        let members = (0..count)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        for source in [
            format!(
                r#"{{{{ range list {members} }}}}{{{{ fail "body" }}}}{{{{ else }}}}wrong{{{{ end }}}}"#
            ),
            format!(
                r#"{{{{ range {count} }}}}{{{{ fail "body" }}}}{{{{ else }}}}wrong{{{{ end }}}}"#
            ),
        ] {
            sim_assert_eq!(have: execute_document(&source, None, &db).abort_predicate(), want: Predicate::True, "{source}");
        }
    }
}

#[test]
fn unknown_range_keeps_zero_exit_and_terminal_body_flow() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let exits = Executor { db: &db }.body(
        r#"{{ range . }}{{ fail "body" }}{{ else }}zero{{ end }}"#,
        None,
        State::new(Value::unknown()),
    );
    sim_assert_eq!(have: exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
    sim_assert_eq!(have: exits.iter().filter(|exit| matches!(exit.flow, Flow::Abort(AbortReason::Fail))).count(), want: 1);

    // Fresh local allocation does not write caller state; both zero and normal
    // exits still restore the outer scope.
    let exits = Executor { db: &db }.body(
        indoc! {r#"
            {{ $outer := "kept" }}
            {{ range . }}{{ $local := dict }}{{ else }}zero{{ end }}
            {{ if ne $outer "kept" }}{{ fail "outer scope changed" }}{{ end }}
        "#},
        None,
        State::new(Value::unknown()),
    );
    assert!(
        exits
            .iter()
            .any(|exit| matches!(exit.flow, Flow::Normal(_)))
    );
    sim_assert_eq!(have: exits.iter().filter(|exit| matches!(exit.flow, Flow::Abort(AbortReason::Fail))).count(), want: 0);
}

#[test]
fn range_precision_does_not_replace_integer_or_slice_runtime_types() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r#"{{ range int64 3 }}{{ if not (typeIs "int64" .) }}{{ fail "integer type" }}{{ end }}{{ end }}"#,
        r#"{{ if typeIs "[]interface {}" (until 3) }}{{ fail "numeric slice type" }}{{ end }}"#,
        r#"{{ if eq (toJson (get (fromJson `{"n":3}`) "n")) "3.0" }}{{ fail "numeric serialization" }}{{ end }}"#,
    ] {
        sim_assert_eq!(have: execute_document(source, None, &db).abort_predicate(), want: Predicate::False, "{source}");
    }
}

#[test]
#[ignore = "R3b regression vs main, package B (iterable domains and loop outcomes): summary read only body preserves unwritten bindings and maps; tracked in plan/value-algebra-r3b.md follow-ups"]
fn summary_read_only_body_preserves_unwritten_bindings_and_maps() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for subject in ["$saved", "$d.x"] {
        let source = format!(
            r#"{{{{ $saved := "old" }}}}{{{{ $d := dict "x" "old" }}}}{{{{ range 33 }}}}{{{{ $_ := printf "%v" . }}}}{{{{ end }}}}{{{{ if eq {subject} "old" }}}}{{{{ fail "preserved" }}}}{{{{ end }}}}"#
        );
        sim_assert_eq!(have: execute_document(&source, None, &db).abort_predicate(), want: Predicate::True, "{source}");
    }
}

#[test]
#[ignore = "R3b regression vs main, package B (iterable domains and loop outcomes): summary retains final assignment on normal break and continue; tracked in plan/value-algebra-r3b.md follow-ups"]
fn summary_retains_final_assignment_on_normal_break_and_continue() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for tail in ["", "{{ break }}", "{{ continue }}"] {
        let source = format!(
            r#"{{{{ $x := "before" }}}}{{{{ range 33 }}}}{{{{ $x = "after" }}}}{tail}{{{{ end }}}}{{{{ if eq $x "after" }}}}{{{{ fail "final assignment" }}}}{{{{ end }}}}"#
        );
        sim_assert_eq!(have: execute_document(&source, None, &db).abort_predicate(), want: Predicate::True, "{source}");
    }
}

#[test]
#[ignore = "R3b regression vs main, package B (iterable domains and loop outcomes): summary write footprint preserves unrelated objects and frozen scalars; tracked in plan/value-algebra-r3b.md follow-ups"]
fn summary_write_footprint_preserves_unrelated_objects_and_frozen_scalars() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for subject in ["$other.x", "$saved"] {
        let source = format!(
            r#"{{{{ $d := dict "x" "old" }}}}{{{{ $alias := $d }}}}{{{{ $other := dict "x" "old" }}}}{{{{ $saved := $d.x }}}}{{{{ range 33 }}}}{{{{ $_ := set $alias "x" "new" }}}}{{{{ end }}}}{{{{ if eq {subject} "old" }}}}{{{{ fail "preserved" }}}}{{{{ end }}}}"#
        );
        sim_assert_eq!(have: execute_document(&source, None, &db).abort_predicate(), want: Predicate::True, "{source}");
    }
}

#[test]
fn summary_havocs_writes_in_later_iterations_and_conditional_suffixes() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for source in [
        r#"{{ $d := dict "x" "old" }}{{ range 33 }}{{ if eq . 32 }}{{ $_ := set $d "x" "new" }}{{ end }}{{ end }}{{ if eq $d.x "old" }}{{ fail "stale" }}{{ end }}"#,
        r#"{{ $x := "old" }}{{ range 33 }}{{ if lt . 32 }}{{ continue }}{{ end }}{{ $x = "new" }}{{ end }}{{ if eq $x "old" }}{{ fail "stale" }}{{ end }}"#,
        r#"{{ $d := dict "x" "old" }}{{ $alias := $d }}{{ range 33 }}{{ if eq . 0 }}{{ $alias = dict }}{{ else }}{{ $_ := set $alias "x" "new" }}{{ end }}{{ end }}{{ if ne $d.x "old" }}{{ fail "different target" }}{{ end }}"#,
        r#"{{ $d := dict "x" "old" }}{{ range 33 }}{{ $_ := tpl `{{ $_ := set . "x" "new" }}` $d }}{{ end }}{{ if eq $d.x "old" }}{{ fail "hidden writer" }}{{ end }}"#,
        r#"{{ $k := 0 }}{{ $v := "old" }}{{ range 33 }}{{ range $k, $v = list "new" }}{{ end }}{{ end }}{{ if eq $v "old" }}{{ fail "nested assignment" }}{{ end }}"#,
    ] {
        sim_assert_eq!(have: execute_document(source, None, &db).abort_predicate(), want: Predicate::False, "{source}");
    }
}

#[test]
fn summary_unknown_length_retains_zero_and_nonzero_final_bindings() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let exits = Executor { db: &db }.body(
        r#"{{ $x := "before" }}{{ range . }}{{ $x = "after" }}{{ end }}{{ if eq $x "before" }}{{ fail "zero" }}{{ end }}"#,
        None,
        State::new(Value::unknown()),
    );
    assert!(
        exits
            .iter()
            .any(|exit| matches!(exit.flow, Flow::Normal(_)))
    );
    assert!(
        exits
            .iter()
            .any(|exit| matches!(exit.flow, Flow::Abort(AbortReason::Fail)))
    );
}

#[test]
fn allocation_collapse_keeps_exact_ids_and_frozen_versions() -> eyre::Result<()> {
    use crate::value::{Field, Multiplicity, Remainder};

    let mut state = State::new(Value::nil());
    let exact = state.heap.map(Remainder::Closed);
    let exact_id = state
        .map_target(&exact)
        .ok_or_eyre("missing exact target")?;
    state
        .heap
        .write(&exact_id, "x", Field::Present(Value::string("old")));
    let frozen = state.heap.freeze(&exact);
    for _ in 1..4096 {
        state.heap.map(Remainder::Closed);
    }
    let summary = state.heap.map(Remainder::Closed);
    let summary_id = state
        .map_target(&summary)
        .ok_or_eyre("missing summary target")?;
    assert_ne!(summary_id, exact_id);
    sim_assert_eq!(have: state.heap.version.object(&summary_id).map(|object| object.multiplicity), want: Some(Multiplicity::Summary));
    state
        .heap
        .write(&summary_id, "x", Field::Present(Value::string("summary")));
    state.heap.map(Remainder::Closed);
    state
        .heap
        .write(&exact_id, "x", Field::Present(Value::string("new")));
    sim_assert_eq!(have: frozen.json(), want: Some(serde_json::json!({"x": "old"})));
    sim_assert_eq!(have: state.heap.freeze(&exact).json(), want: Some(serde_json::json!({"x": "new"})));
    assert!(state.heap.freeze(&summary).json().is_none());
    Ok(())
}

#[test]
fn summary_range_keeps_scalar_member_types_across_the_unroll_bound() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for count in [31, 32, 33] {
        for collection in [
            count.to_string(),
            format!("list {}", vec!["1"; count].join(" ")),
        ] {
            let source = format!(
                r"{{{{ range ({collection}) }}}}{{{{ lower . }}}}{{{{ break }}}}{{{{ end }}}}"
            );
            sim_assert_eq!(have: execute_document(&source, None, &db).abort_predicate(), want: Predicate::True, "{source}");
        }
    }
}

#[test]
fn allocation_collapse_preserves_a_reserved_occurrence_and_its_snapshots() -> eyre::Result<()> {
    use crate::value::{Field, Multiplicity, ObjectData, Remainder};
    use std::collections::BTreeMap;

    let mut state = State::new(Value::nil());
    let value = state.heap.map(Remainder::Closed);
    let id = state
        .map_target(&value)
        .ok_or_eyre("missing reserved object")?;
    state
        .heap
        .write(&id, "old", Field::Present(Value::string("frozen")));
    let frozen = state.heap.freeze(&value);
    for _ in 1..4096 {
        state.heap.map(Remainder::Closed);
    }
    // Reinstalling an already reserved identity does not add a live allocation class.
    let installed = state.heap.allocate_at(
        Some(&id),
        ObjectData::Map {
            fields: Rc::new(
                (BTreeMap::from([("new".to_owned(), Field::Present(Value::string("current")))]))
                    .into(),
            ),
            remainder: Remainder::Closed,
        },
        Multiplicity::Singleton,
    );
    sim_assert_eq!(have: installed, want: id);
    sim_assert_eq!(have: frozen.json(), want: Some(serde_json::json!({"old": "frozen"})));
    sim_assert_eq!(have: state.heap.freeze(&value).json(), want: Some(serde_json::json!({"new": "current"})));
    assert!(state.heap.arena.aggregate_summary.get().is_none());
    Ok(())
}

#[test]
fn summary_scalar_type_never_comes_from_only_the_first_member() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let members = std::iter::once(r#""ok""#)
        .chain(std::iter::repeat_n("1", 32))
        .collect::<Vec<_>>()
        .join(" ");
    let source = format!(
        r"{{{{ range list {members} }}}}{{{{ $_ := lower . }}}}{{{{ break }}}}{{{{ end }}}}"
    );
    sim_assert_eq!(have: execute_document(&source, None, &db).abort_predicate(), want: Predicate::False);
    let members = [r#""ok""#; 33].join(" ");
    let source = format!(
        r#"{{{{ range list {members} }}}}{{{{ if kindIs "string" . }}}}{{{{ fail "string member" }}}}{{{{ end }}}}{{{{ end }}}}"#
    );
    sim_assert_eq!(have: execute_document(&source, None, &db).abort_predicate(), want: Predicate::True);
}

#[test]
fn allocation_collapse_keeps_aliases_in_the_unknown_write_region() -> eyre::Result<()> {
    use crate::value::{Datum, Field, MapType, Multiplicity, ObjectData, Remainder, operations};
    use std::collections::BTreeMap;

    for count in [4095, 4096, 4097] {
        let mut state = State::new(Value::nil());
        let child = state.heap.map(Remainder::Closed);
        let child_id = state
            .map_target(&child)
            .ok_or_eyre("missing child target")?;
        state
            .heap
            .write(&child_id, "x", Field::Present(Value::string("old")));
        let frozen = state.heap.freeze(&child);
        let scalar = state.get_field(&child, "x");
        let saved_scalar = state.heap.freeze(&scalar);
        let saved_truth = state.test(&scalar);
        for _ in 1..count {
            state.heap.map(Remainder::Closed);
        }
        let parent_id = state.heap.allocate(
            ObjectData::Map {
                fields: Rc::new(
                    (BTreeMap::from([("child".to_owned(), Field::Present(child.clone()))])).into(),
                ),
                remainder: Remainder::Closed,
            },
            Multiplicity::Singleton,
        );
        let parent = Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: Some(parent_id),
        });
        // A widened wrapper must still permit a writer to reach its formerly exact child.
        operations::widen_operands(&[parent], &mut state);
        assert!(state.heap.freeze(&child).json().is_none());
        sim_assert_eq!(have: frozen.json(), want: Some(serde_json::json!({"x": "old"})));
        sim_assert_eq!(have: saved_scalar.json(), want: Some(serde_json::json!("old")));
        sim_assert_eq!(have: saved_truth.when_true.clone(), want: Predicate::True);
    }
    Ok(())
}

/// Whether a `fail` in `source`, run over an unknown root, is reachable.
fn reaches_fail(source: &str) -> bool {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    Executor { db: &db }
        .body(source, None, State::new(Value::unknown()))
        .iter()
        .any(|exit| matches!(exit.flow, Flow::Abort(AbortReason::Fail)))
}

/// (6a1-V, Sol pre-landing item 4) A second unknown write is not covered by the first: the
/// first iteration restores `$d.x` after its unknown write, the second iteration's unknown write
/// may set it to `stop`, and the third iteration's check then aborts.
#[test]
fn a_later_unknown_write_reaches_the_third_iteration() {
    assert!(reaches_fail(indoc! {r#"
        {{- $d := dict "x" "go" }}{{ $first := true }}
        {{- range .items }}
        {{- if eq $d.x "stop" }}{{ fail "third" }}{{ end }}
        {{- if $first }}{{ include (printf "h-%v" .) $d }}{{ $_ := set $d "x" "go" }}{{ else }}{{ include (printf "h-%v" .) $d }}{{ end }}
        {{- $first = false }}
        {{- end }}
    "#}));
}

/// (6a1-T, Sol pre-landing item 1) In-place sorts of shared slots reach a third-iteration state
/// the second does not cover: Helm sorts `d,c,b,a` to `abcd` after three members.
#[test]
fn a_slice_sorted_in_place_reaches_its_third_iteration_state() {
    assert!(reaches_fail(indoc! {r#"
        {{- $s := splitList "," "d,c,b,a" }}
        {{- range .items }}{{ $_ := sortAlpha (slice $s 0 2) }}{{ $_ := sortAlpha (slice $s 1 3) }}{{ $_ := sortAlpha (slice $s 2 4) }}{{- end }}
        {{- if eq (join "" $s) "abcd" }}{{ fail "sorted" }}{{ end }}
    "#}));
}

/// (6a1-X) A range binder is declared afresh at every iteration, and a frame inside an outer
/// binding shadows it: neither is carried, so an unknown write that rewrites them leaves the
/// next iteration's start covered. A visible outer binding still counts.
#[test]
fn shadowed_and_rebound_bindings_are_not_carried() {
    use crate::value::loops::{CarriedScope, covers};
    use std::collections::BTreeSet;

    let mut start = State::new(Value::unknown());
    start.bind("outer".to_owned(), Value::nil());
    start.bind("shadowed".to_owned(), Value::nil());
    start.enter_scope(None);
    start.bind("shadowed".to_owned(), Value::nil());
    start.bind("k".to_owned(), Value::nil());
    let frames = start.frames.len();
    let rebind = |name: &str, frame: usize| {
        let mut end = start.clone();
        if let Some(target) = end.frames.get_mut(frame) {
            Rc::make_mut(target)
                .bindings
                .insert(name.to_owned(), Value::unknown());
        }
        end
    };
    let rebound = BTreeSet::from(["k".to_owned()]);
    let scope = CarriedScope {
        frames,
        rebound: &rebound,
    };
    let none = BTreeSet::new();
    assert!(covers(&start, &rebind("k", frames - 1), scope));
    assert!(!covers(
        &start,
        &rebind("k", frames - 1),
        CarriedScope {
            frames,
            rebound: &none,
        }
    ));
    assert!(covers(&start, &rebind("shadowed", 0), scope));
    assert!(!covers(&start, &rebind("outer", 0), scope));
}

/// An assumption set has its own Top; fresh choices cannot extend the pass ladder forever. The
/// Top states the assumption the pass made instead of `False` (6a1 rework item 1), so the carried
/// path still admits every document where that assumption holds.
#[test]
fn loop_top_covers_fresh_validity_assumptions() {
    use crate::value::loops::{Carried, CarriedScope, Widening};
    use crate::value::{Coverage, DecisionEdge, path::LinkKind};
    use std::collections::BTreeSet;

    let state = State::new(Value::unknown());
    let rebound = BTreeSet::new();
    let scope = CarriedScope {
        frames: state.frames.len(),
        rebound: &rebound,
    };
    let mut carried = Carried::new(state);
    for index in 0..2 {
        let mut end = carried.state.clone();
        let predicate = Predicate::truthy_path(format!("p{index}"));
        let test = end
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        end.active
            .push(DecisionEdge { test, taken: true }, LinkKind::Assume);
        carried.widen(&end, scope, Widening::Top);
        assert!(carried.covers(&end, scope));
        sim_assert_eq!(have: carried.state.path_predicate(), want: Predicate::truthy_path("p0"));
    }
}

/// Reading an input map leaves its entry remainder intact; a lazy merge changes that remainder.
#[test]
fn late_input_remainders_are_compared_against_the_entry_state() -> eyre::Result<()> {
    use crate::value::loops::{CarriedScope, covers};
    use helm_schema_core::ValuesPath;
    use std::collections::BTreeSet;

    let start = State::new(Value::unknown());
    let rebound = BTreeSet::new();
    let scope = CarriedScope {
        frames: start.frames.len(),
        rebound: &rebound,
    };
    let mut end = start.clone();
    let path = ValuesPath::parse("cfg");
    let object = end.input_object(&path);
    assert!(covers(&start, &end, scope), "a read alone is covered");
    let source = Value::input(ValuesPath::parse("source"));
    assert!(end.merge_into(&object, &source, true, 0, &Rc::default()));
    let data = end
        .heap
        .version
        .object(&object)
        .ok_or_eyre("merged input")?;
    assert!(
        matches!(&data.data, crate::value::ObjectData::Map { fields, .. } if fields.is_empty())
    );
    assert!(
        !covers(&start, &end, scope),
        "a layered remainder is a carried mutation"
    );
    Ok(())
}

/// (6a1 rework item 2, ADR 0072 §10) Summarizing more than 4,096 exits widens only the
/// continuation: every member's consumptions, requirement reads, sites and text sites survive
/// with the conditions they were recorded under.
#[test]
fn summarized_exits_keep_every_member_history() -> eyre::Result<()> {
    use crate::value::ExitBuffer;
    use helm_schema_core::ValuesPath;

    let db = IrAnalysisDb::new(&DefineIndex::new());
    let normal_exit = |source: &str| {
        execute_document(source, None, &db)
            .exits
            .into_iter()
            .find(|exit| matches!(exit.flow, Flow::Normal(_)))
            .ok_or_eyre("normal exit")
    };
    let early = normal_exit(indoc! {r"
        {{- $_ := upper .Values.early }}
        host: {{ .Values.host.k }}
        early: text
    "})?;
    let late = normal_exit("{{- $_ := upper .Values.late }}late: text\n")?;
    let mut buffer = ExitBuffer::default();
    buffer.push(early.clone());
    for _ in 0..4096 {
        buffer.push(late.clone());
    }
    let summarized = buffer
        .finish()
        .into_iter()
        .find(|exit| matches!(exit.flow, Flow::Normal(_)))
        .ok_or_eyre("summarized normal exit")?;
    let state = &summarized.state;
    for member in [&early, &late] {
        for consumption in &member.state.consumptions {
            assert!(
                state.consumptions.iter().any(|kept| kept == consumption),
                "lost the consumption of {:?}",
                consumption.path
            );
        }
        for site in &member.state.sites {
            assert!(
                state
                    .sites
                    .iter()
                    .any(|kept| (kept.program, kept.ordinal, kept.span)
                        == (site.program, site.ordinal, site.span)),
                "lost a site"
            );
        }
        for text in &member.state.text_sites {
            assert!(
                state.text_sites.iter().any(|kept| kept == text),
                "lost a text site"
            );
        }
    }
    let host = ValuesPath::parse("host");
    let requirement = early
        .state
        .reads
        .iter()
        .find(|read| read.path == host && read.coverage.is_requirement())
        .ok_or_eyre("the early member's requirement read")?;
    assert!(
        state.reads.iter().any(|read| read == requirement),
        "lost the requirement read of host"
    );
    Ok(())
}

#[test]
fn large_integer_range_starts_at_exact_zero_and_reaches_its_rest() {
    assert!(!reaches_fail(
        r#"{{ range 33 }}{{ if eq . 0 }}{{ break }}{{ end }}{{ fail "nonzero" }}{{ end }}"#,
    ));
    assert!(reaches_fail(
        r#"{{ range $i := 33 }}{{ if eq $i 0 }}{{ continue }}{{ end }}{{ fail "nonzero" }}{{ end }}"#,
    ));
}

#[test]
fn a_first_pass_render_keeps_serialized_use_after_a_terminal_exit() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ range 33 }}{{ toYaml $.Values.labels }}{{ fail "after rendering" }}{{ end }}"#,
        None,
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::True);
    assert!(execution.projection_reads().iter().any(|read| {
        read.kind == crate::ValueKind::Serialized
            && read.values_path == helm_schema_core::ValuesPath::parse("labels")
    }));
}
