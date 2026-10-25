use crate::analysis_db::IrAnalysisDb;
use crate::value::execute_document;
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

fn texts(source: &str) -> Vec<Option<String>> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    execute_document(source, Some("templates/test.yaml"), &db)
        .text()
        .map(|(_, text)| text.exact_text())
        .collect()
}

#[test]
fn consumed_scalar_and_serialization_survive_a_later_alias_write() {
    sim_assert_eq!(
        have: texts(r#"{{ $d := dict "x" "old" }}{{ $before := toJson $d }}{{ $alias := $d }}{{ $_ := set $alias "x" "new" }}{{ $before }}|{{ get $d "x" }}"#),
        want: vec![Some(r#"{"x":"old"}|new"#.to_owned())]
    );
}

#[test]
fn short_circuit_stops_the_unreachable_mutation() {
    sim_assert_eq!(
        have: texts(r#"{{ $d := dict "x" "old" }}{{ $_ := and false (set $d "x" "new") }}{{ get $d "x" }}"#),
        want: vec![Some("old".to_owned())]
    );
}

#[test]
fn include_uses_caller_heap_and_has_an_isolated_binding_frame() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "templates/_helpers.tpl",
        r#"{{ define "write" }}{{ $_ := set . "x" "new" }}{{ get . "x" }}{{ end }}"#,
    );
    let db = IrAnalysisDb::new(&defines);
    let execution = execute_document(
        r#"{{ $d := dict "x" "old" }}{{ $caller := "kept" }}{{ include "write" $d }}|{{ get $d "x" }}|{{ $caller }}"#,
        Some("templates/test.yaml"),
        &db,
    );
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("new|new|kept".to_owned())]);
}

#[test]
fn overlapping_unknown_success_prevents_a_universal_abort() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ if eq (randAlphaNum 1) "a" }}{{ fail "no" }}{{ else }}yes{{ end }}"#,
        Some("templates/test.yaml"),
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("yes".to_owned())]);
}

#[test]
fn aborted_output_does_not_publish_a_completed_render_site() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"before{{ "placed" }}{{ fail "no" }}"#,
        Some("templates/test.yaml"),
        &db,
    );
    sim_assert_eq!(have: execution.sites().count(), want: 0);
    sim_assert_eq!(have: execution.text().count(), want: 0);
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::True);
}

#[test]
fn a_fresh_single_source_call_does_not_inherit_the_previous_heap() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let first = execute_document(
        r#"{{ $_ := set .Values "x" "new" }}{{ .Values.x }}"#,
        None,
        &db,
    );
    sim_assert_eq!(have: first.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("new".to_owned())]);
    let second = execute_document("{{ .Values.x }}", None, &db);
    sim_assert_eq!(have: second.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![None]);
}

#[test]
fn live_string_slot_is_copied_at_call_consumption_and_emission() {
    sim_assert_eq!(
        have: texts(r#"{{ $s := toStrings (list "b" "a") }}{{ $slot := index $s 0 }}{{ $slot }}|{{ ternary "fallback" $slot (empty (sortAlpha $s)) }}|{{ $slot }}"#),
        want: vec![Some("b|a|a".to_owned())]
    );
}

#[test]
fn text_budget_neighbors_preserve_prior_exact_text_and_widen_the_suffix() {
    for count in [127, 128, 129] {
        let source = r#"{{ "x" }}"#.repeat(count);
        let expected = if count <= 128 {
            Some("x".repeat(count))
        } else {
            None
        };
        sim_assert_eq!(have: texts(&source), want: vec![expected]);
    }
}

#[test]
fn deep_copy_of_an_unmaterialized_input_cannot_alias_the_original() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ $copy := deepCopy .Values.obj }}{{ $_ := set $copy "x" "new" }}{{ if eq .Values.obj.x "new" }}{{ fail "bad" }}{{ end }}"#,
        None,
        &db,
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
}

#[test]
fn object_and_field_caps_keep_older_frozen_versions() -> eyre::Result<()> {
    use super::{Datum, Field, Heap, Leaf, Remainder, Value, ValueNode};
    let mut heap = Heap::new();
    let value = heap.map(Remainder::Closed);
    let ValueNode::Leaf(Leaf {
        datum: Datum::Map {
            target: Some(id),
            view: _,
        },
        origin: _,
    }) = value.node.as_ref()
    else {
        eyre::bail!("map allocation did not produce a target");
    };
    heap.write(id, "saved", Field::Present(Value::string("old")));
    let frozen = heap.freeze(&value);
    for ordinal in 0..129 {
        heap.write(
            id,
            &format!("field{ordinal}"),
            Field::Present(Value::string("value")),
        );
    }
    for _ in 0..4200 {
        heap.map(Remainder::Closed);
    }
    sim_assert_eq!(have: heap.version.pages.values().map(|page| page.len()).sum::<usize>(), want: 4097);
    let saved = frozen
        .read_field("saved")
        .present()
        .ok_or_eyre("saved snapshot field disappeared")?;
    sim_assert_eq!(have: saved.literal(), want: Some(&helm_schema_core::GuardValue::String("old".to_owned())));
    Ok(())
}

#[test]
fn unknown_render_region_reads_keep_root_coverage_without_raw_identity() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ $_ := merge .Values (dict "choose" true) }}{{ if .Values.choose }}yes{{ end }}"#,
        None,
        &db,
    );
    assert!(
        execution
            .unknown_read_regions()
            .any(|path| path.segments().next().is_none())
    );
    assert!(
        execution
            .all_sites()
            .filter(|site| !site.rendered())
            .all(|site| site.value().identity().is_none())
    );
}

#[test]
fn transformed_scalar_field_reads_do_not_invent_raw_child_paths() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document("{{ (fromJson .Values.text).choose }}", None, &db);
    assert!(
        execution
            .reads()
            .iter()
            .any(|read| read.values_path == helm_schema_core::ValuesPath::parse("text"))
    );
    assert!(
        execution
            .reads()
            .iter()
            .all(|read| read.values_path != helm_schema_core::ValuesPath::parse("text.choose"))
    );
}

#[test]
fn alternative_collapse_widens_frame_and_dot_aliases_with_all_inputs() -> eyre::Result<()> {
    use super::{Datum, Exit, Field, Flow, Leaf, Normal, Remainder, State, Value, ValueNode};
    let mut state = State::new(Value::nil());
    let first = state.heap.map(Remainder::Closed);
    let second = state.heap.map(Remainder::Closed);
    let first_target = state
        .map_target(&first)
        .ok_or_eyre("first target missing")?;
    let second_target = state
        .map_target(&second)
        .ok_or_eyre("second target missing")?;
    state.heap.write(
        &first_target,
        "value",
        Field::Present(Value::input(helm_schema_core::ValuesPath::parse("first"))),
    );
    state.heap.write(
        &second_target,
        "value",
        Field::Present(Value::input(helm_schema_core::ValuesPath::parse("second"))),
    );
    state.bind("first".to_owned(), first.clone());
    state.bind("second".to_owned(), second.clone());
    let exits = (0..4097)
        .map(|ordinal| {
            let mut state = state.clone();
            state.assign(
                "second",
                if ordinal % 2 == 0 {
                    first.clone()
                } else {
                    second.clone()
                },
            );
            Exit {
                state,
                flow: Flow::Normal(Normal {
                    value: first.clone(),
                }),
            }
        })
        .collect();
    let collapsed = super::control::bound_exits(exits);
    sim_assert_eq!(have: collapsed.len(), want: 1);
    let exit = collapsed.first().ok_or_eyre("collapsed exit missing")?;
    for value in [
        exit.state.read_local("first"),
        exit.state.read_local("second"),
        exit.state.dot(),
    ] {
        let value = value.ok_or_eyre("frame value missing after collapse")?;
        assert!(matches!(
            value.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(_),
                origin: _
            })
        ));
        assert!(
            value
                .inputs()
                .contains(&helm_schema_core::ValuesPath::parse("first"))
        );
        assert!(
            value
                .inputs()
                .contains(&helm_schema_core::ValuesPath::parse("second"))
        );
    }
    Ok(())
}

#[test]
fn overflow_buffer_summarizes_the_entire_tail_without_losing_flow_classes() -> eyre::Result<()> {
    use super::{
        AbortReason, Datum, Exit, ExitBuffer, Flow, Leaf, Normal, ReadCoverage, RuntimeRead, State,
        Value, ValueNode,
    };
    let mut state = State::new(Value::nil());
    state.bind("kept".to_owned(), Value::string("before"));
    let mut buffer = ExitBuffer::default();
    for _ in 0..4097 {
        buffer.push(Exit {
            state: state.clone(),
            flow: Flow::Normal(Normal {
                value: Value::string("prefix"),
            }),
        });
    }
    let mut late = state.clone();
    late.assign(
        "kept",
        Value::input(helm_schema_core::ValuesPath::parse("late")),
    );
    late.reads.insert(RuntimeRead {
        coverage: ReadCoverage::ExactPath,
        path: helm_schema_core::ValuesPath::parse("tail_read"),
        condition: Predicate::True,
    });
    buffer.push(Exit {
        state: late,
        flow: Flow::Normal(Normal {
            value: Value::input(helm_schema_core::ValuesPath::parse("late")),
        }),
    });
    let target = state.loop_id(7).ok_or_eyre("loop identity missing")?;
    buffer.push(Exit {
        state: state.clone(),
        flow: Flow::Abort(AbortReason::Fail),
    });
    buffer.push(Exit {
        state: state.clone(),
        flow: Flow::Break(target),
    });
    buffer.push(Exit {
        state,
        flow: Flow::Continue(target),
    });
    let exits = buffer.finish();
    sim_assert_eq!(have: exits.len(), want: 4);
    sim_assert_eq!(have: exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
    let normal = exits
        .iter()
        .find(|exit| matches!(exit.flow, Flow::Normal(_)))
        .ok_or_eyre("normal summary missing")?;
    let Flow::Normal(value) = &normal.flow else {
        eyre::bail!("normal summary changed flow");
    };
    assert!(matches!(
        value.value.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: Datum::Unknown(_),
            origin: _
        })
    ));
    assert!(
        value
            .value
            .inputs()
            .contains(&helm_schema_core::ValuesPath::parse("late"))
    );
    assert!(normal.state.reads.iter().any(|read| read.path
        == helm_schema_core::ValuesPath::parse("tail_read")
        && read.condition == Predicate::True));
    let kept = normal
        .state
        .read_local("kept")
        .ok_or_eyre("lexical binding missing")?;
    assert!(matches!(
        kept.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: Datum::Unknown(_),
            origin: _
        })
    ));
    assert!(exits.iter().any(|exit| matches!(exit.flow, Flow::Abort(_))));
    assert!(
        exits
            .iter()
            .any(|exit| matches!(exit.flow, Flow::Break(found) if found == target))
    );
    assert!(
        exits
            .iter()
            .any(|exit| matches!(exit.flow, Flow::Continue(found) if found == target))
    );
    sim_assert_eq!(have: super::DocumentExecution::new(exits).abort_predicate(), want: Predicate::False);
    Ok(())
}

#[test]
fn reconverged_span_does_not_drop_the_missing_conditional_suffix() {
    let source = "prefix{{ if .Values.flag }}suffix{{ end }}";
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, None, &db);
    let span = helm_schema_syntax::Span::new(0, source.len());
    let outcomes = execution.text_for_span(span, None).collect::<Vec<_>>();

    // The false arm renders "prefix" and the true arm renders "prefixsuffix".
    // A consumer that discards guards must abstain rather than report only the longer string.
    assert!(!outcomes.is_empty());
    assert!(outcomes.iter().all(|(_, text)| text.is_none()));
}

#[test]
fn reconverged_api_version_suffix_never_becomes_the_only_literal() -> eyre::Result<()> {
    let source = indoc::indoc! {r"
        apiVersion: apps/v1{{ if .Values.legacy }}beta1{{ end }}
        kind: Deployment
        metadata:
          name: example
    "};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, None, &db);
    let end = source
        .find('\n')
        .ok_or_eyre("api version line is missing")?;
    let span = helm_schema_syntax::Span::new("apiVersion: ".len(), end);

    // Both apps/v1 and apps/v1beta1 are feasible scalar outputs.
    // The current literal-only resource consumer cannot discard the empty-suffix alternative.
    let body = crate::resource_identity::executed_scalar_body(&execution, span, None);
    sim_assert_eq!(have: body.all_literals(), want: Vec::<String>::new());
    Ok(())
}

mod joined_query_cases {
    use std::collections::BTreeSet;

    use color_eyre::eyre::{self, OptionExt as _};
    use helm_schema_ast::DefineIndex;
    use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
    use helm_schema_syntax::Span;
    use test_util::prelude::sim_assert_eq;

    fn execute(source: &str) -> crate::value::DocumentExecution {
        let defines = DefineIndex::new();
        let db = crate::analysis_db::IrAnalysisDb::new(&defines);
        crate::value::execute_document(source, None, &db)
    }

    #[test]
    fn joined_input_field_retains_untouched_origin_tail() {
        let execution = execute(
            r#"{{ if .Values.choose }}{{ $_ := set .Values "x" "new" }}{{ end }}{{ .Values.x }}"#,
        );
        let mut literals = BTreeSet::new();
        let mut identities = BTreeSet::new();
        let mut outputs = 0;
        for site in execution.sites().filter(|site| site.rendered()) {
            outputs += 1;
            for arm in site.value().alternatives() {
                assert!(site.coverage_exact() && arm.coverage_exact());
                let condition =
                    Predicate::all(vec![site.condition().clone(), arm.condition().clone()])
                        .normalize_boolean();
                if let Some(literal) = arm.value().literal() {
                    literals.insert((condition.clone(), literal.clone()));
                }
                if let Some(identity) = arm.value().identity() {
                    identities.insert((condition, identity.clone()));
                }
            }
        }
        assert!(outputs > 0);
        sim_assert_eq!(have: literals, want: BTreeSet::from([(Predicate::truthy_path("choose"), GuardValue::string("new"))]));
        sim_assert_eq!(have: identities, want: BTreeSet::from([(Predicate::truthy_path("choose").negated(), ValuesPath::parse("x"))]));
    }

    fn present_condition(prefix: &str) -> Predicate {
        let mut source = format!("{prefix}{{{{ if hasKey .Values \"x\" }}}}");
        let start = source.len();
        source.push_str("present");
        let span = Span::new(start, source.len());
        source.push_str("{{ else }}absent{{ end }}");
        execute(&source).condition_for_span(span, None)
    }

    #[test]
    fn joined_set_presence_keeps_writer_condition() {
        let raw_present = Predicate::from(Guard::Absent {
            path: ValuesPath::parse("x"),
        })
        .negated();
        sim_assert_eq!(
            have: present_condition(r#"{{ if .Values.choose }}{{ $_ := set .Values "x" "new" }}{{ end }}"#),
            want: Predicate::Or(vec![Predicate::truthy_path("choose"), raw_present]).normalize_boolean()
        );
    }

    #[test]
    fn joined_unset_presence_keeps_writer_condition() {
        let raw_present = Predicate::from(Guard::Absent {
            path: ValuesPath::parse("x"),
        })
        .negated();
        sim_assert_eq!(
            have: present_condition(r#"{{ if .Values.choose }}{{ $_ := unset .Values "x" }}{{ end }}"#),
            want: Predicate::all(vec![Predicate::truthy_path("choose").negated(), raw_present]).normalize_boolean()
        );
    }

    #[test]
    fn open_input_remainder_never_recovers_child_identity() -> eyre::Result<()> {
        let execution = execute(
            r#"{{ $_ := set .Values (printf "%s" .Values.key) "changed" }}{{ if .Values.choose }}{{ $_ := set .Values "x" "new" }}{{ end }}{{ .Values.x }}"#,
        );
        let site = execution
            .sites()
            .filter(|site| site.rendered())
            .last()
            .ok_or_eyre("missing open remainder output")?;
        assert!(!site.value().inputs().is_empty());
        assert!(
            site.value()
                .alternatives()
                .iter()
                .all(|arm| arm.value().identity().is_none())
        );
        Ok(())
    }

    #[test]
    fn joined_get_keeps_conditional_assigned_value_and_original_influence() {
        let execution = execute(
            r#"{{ if .Values.choose }}{{ $_ := set .Values "x" "new" }}{{ end }}{{ get .Values "x" }}"#,
        );
        let mut literals = BTreeSet::new();
        let mut inputs = BTreeSet::new();
        for site in execution.sites().filter(|site| site.rendered()) {
            inputs.extend(site.value().inputs());
            for arm in site.value().alternatives() {
                if let Some(literal) = arm.value().literal() {
                    literals.insert((
                        Predicate::all(vec![site.condition().clone(), arm.condition().clone()])
                            .normalize_boolean(),
                        literal.clone(),
                    ));
                }
            }
        }
        sim_assert_eq!(have: literals, want: BTreeSet::from([(Predicate::truthy_path("choose"), GuardValue::string("new"))]));
        assert!(inputs.contains(&ValuesPath::parse("x")));
        assert!(
            execution
                .reads()
                .iter()
                .any(|read| read.values_path == ValuesPath::parse("x"))
        );
    }

    #[test]
    fn joined_get_maps_conditional_absence_to_empty_string() {
        let execution = execute(
            r#"{{ if .Values.choose }}{{ $_ := unset .Values "x" }}{{ end }}{{ get .Values "x" }}"#,
        );
        let literals = execution
            .sites()
            .filter(|site| site.rendered())
            .flat_map(|site| {
                site.value()
                    .alternatives()
                    .into_iter()
                    .filter_map(move |arm| {
                        arm.value().literal().cloned().map(|literal| {
                            (
                                Predicate::all(vec![
                                    site.condition().clone(),
                                    arm.condition().clone(),
                                ])
                                .normalize_boolean(),
                                literal,
                            )
                        })
                    })
            })
            .collect::<BTreeSet<_>>();
        sim_assert_eq!(have: literals, want: BTreeSet::from([(Predicate::truthy_path("choose"), GuardValue::string(""))]));
    }
}

#[test]
fn sixty_four_independent_controls_reconverge_to_one_normal_state() {
    let source = (0..64)
        .map(|index| format!("{{{{ if .Values.g{index} }}}}item{index}: fixed\n{{{{ end }}}}\n"))
        .collect::<String>();
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(&source, None, &db);
    sim_assert_eq!(have: execution.exits.len(), want: 1);
    assert!(execution.exits.iter().all(|exit| matches!(exit.flow, super::Flow::Normal(_)) && exit.state.active.is_empty()));
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
}

#[test]
fn joined_alias_selection_observes_a_later_shared_object_write() -> eyre::Result<()> {
    use std::collections::BTreeSet;
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ $a := dict "x" "old" }}{{ $b := dict "x" "other" }}{{ $selected := $a }}{{ if .Values.flag }}{{ $selected = $b }}{{ end }}{{ $_ := set $a "x" "new" }}{{ get $selected "x" }}"#,
        None,
        &db,
    );
    sim_assert_eq!(have: execution.exits.len(), want: 1);
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("joined alias output missing")?;
    let literals = site
        .value()
        .alternatives()
        .into_iter()
        .filter_map(|arm| arm.value().literal().cloned())
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(have: literals, want: BTreeSet::from([helm_schema_core::GuardValue::string("new"), helm_schema_core::GuardValue::string("other")]));
    Ok(())
}

#[test]
fn joined_mutation_does_not_change_previously_emitted_scalar_or_serialization() -> eyre::Result<()>
{
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ $d := dict "x" "old" }}{{ $before := toJson $d }}{{ get $d "x" }}|{{ if .Values.flag }}{{ $_ := set $d "x" "yes" }}{{ else }}{{ $_ := set $d "x" "no" }}{{ end }}{{ $before }}|{{ get $d "x" }}"#,
        None,
        &db,
    );
    sim_assert_eq!(have: execution.exits.len(), want: 1);
    let sites = execution
        .sites()
        .filter(|site| site.rendered())
        .collect::<Vec<_>>();
    let before = sites.first().ok_or_eyre("pre-write scalar missing")?;
    sim_assert_eq!(have: before.value().literal(), want: Some(&helm_schema_core::GuardValue::string("old")));
    assert!(sites.iter().any(|site| site.value().literal()
        == Some(&helm_schema_core::GuardValue::string(r#"{"x":"old"}"#))));
    Ok(())
}

#[test]
fn include_conditional_output_preserves_both_equality_guard_directions() {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "templates/_helpers.tpl",
        r#"{{ define "choice" }}{{ if .Values.flag }}alpha{{ else }}beta{{ end }}{{ end }}"#,
    );
    let db = IrAnalysisDb::new(&defines);
    for function in ["eq", "ne"] {
        let source = format!(
            r#"{{{{ $value := include "choice" . }}}}{{{{ if {function} $value "alpha" }}}}{{{{ fail "bad" }}}}{{{{ end }}}}"#
        );
        let execution = execute_document(&source, None, &db);
        let selected = Predicate::truthy_path("flag");
        let expected = if function == "eq" {
            selected
        } else {
            selected.negated()
        };
        sim_assert_eq!(have: execution.abort_predicate(), want: expected.normalize_boolean());
    }
}

#[test]
fn joined_partial_decision_does_not_turn_one_failing_arm_into_universal_failure() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ $value := false }}{{ if mystery }}{{ $value = true }}{{ else }}{{ $value = false }}{{ end }}{{ if $value }}{{ fail "bad" }}{{ else }}okay{{ end }}"#,
        None,
        &db,
    );
    assert!(
        execution
            .exits
            .iter()
            .any(|exit| matches!(exit.flow, super::Flow::Abort(_)))
    );
    assert!(
        execution
            .exits
            .iter()
            .any(|exit| matches!(exit.flow, super::Flow::Normal(_)))
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
}

#[test]
fn nested_one_sided_input_materialization_declines_type_narrowing_join() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        r#"{{ if .Values.flag }}{{ $_ := set .Values.obj "x" "new" }}{{ end }}{{ trim .Values.obj }}"#,
        None,
        &db,
    );
    assert!(
        execution
            .exits
            .iter()
            .any(|exit| matches!(exit.flow, super::Flow::Normal(_)))
    );
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::truthy_path("flag"));
}

#[test]
fn widened_heap_join_keeps_both_sides_transitive_input_dependencies() -> eyre::Result<()> {
    use super::{Coverage, Field, ObjectData, ObjectVersion, Remainder, State, Value};
    use std::collections::BTreeSet;
    use std::rc::Rc;
    let mut state = State::new(Value::nil());
    let outer = state.heap.map(Remainder::Closed);
    let inner = state.heap.map(Remainder::Closed);
    let outer_id = state.map_target(&outer).ok_or_eyre("outer map missing")?;
    let inner_id = state.map_target(&inner).ok_or_eyre("inner map missing")?;
    state.heap.write(
        &inner_id,
        "input",
        Field::Present(Value::input(helm_schema_core::ValuesPath::parse("nested"))),
    );
    state.heap.write(&outer_id, "child", Field::Present(inner));
    state.bind("outer".to_owned(), outer.clone());
    let guard = Predicate::truthy_path("flag");
    let decision = state
        .heap
        .arena
        .decision(guard.clone(), guard.negated(), Coverage::Exact);
    let mut yes = state.fork(Rc::clone(&decision), true);
    let no = state.fork(Rc::clone(&decision), false);
    yes.heap.install(
        outer_id,
        ObjectVersion {
            revision: yes.heap.arena.snapshot(),
            multiplicity: super::Multiplicity::Singleton,
            data: ObjectData::Widened {
                inputs: BTreeSet::from([helm_schema_core::ValuesPath::parse("lost")]),
            },
        },
    );
    let joined = State::join_branches(decision, &yes, &no)
        .ok_or_eyre("compatible widened heap join declined")?;
    let inputs = joined.heap.freeze(&outer).inputs();
    assert!(inputs.contains(&helm_schema_core::ValuesPath::parse("lost")));
    assert!(inputs.contains(&helm_schema_core::ValuesPath::parse("nested")));
    Ok(())
}
mod canonical_under_regressions {
    use std::rc::Rc;

    use color_eyre::eyre::{self, OptionExt as _};
    use helm_schema_core::{GuardValue, Predicate};
    use test_util::prelude::sim_assert_eq;

    use crate::value::{Arena, Coverage, FrozenTest, ScalarType, Value, ValueNode};

    fn decision(arena: &Arena, path: &str) -> Rc<FrozenTest> {
        let positive = Predicate::truthy_path(path);
        arena.decision(positive.clone(), positive.negated(), Coverage::Exact)
    }

    struct Choice<'a> {
        decision: &'a FrozenTest,
        taken: bool,
    }

    fn evaluate(value: &Value, choices: &[Choice<'_>]) -> eyre::Result<GuardValue> {
        let specialized = choices.iter().fold(value.clone(), |value, choice| {
            value.under(choice.decision, choice.taken)
        });
        specialized
            .constant()
            .cloned()
            .ok_or_eyre("specialization did not reach a concrete scalar")
    }

    #[test]
    fn absent_decision_preserves_a_deep_normalized_chain_and_its_sharing() {
        let arena = Arena::default();
        let terminal = Value::scalar(ScalarType::String, None);
        let mut value = terminal.clone();
        let mut decisions = Vec::new();
        for index in 0..64 {
            let test = decision(&arena, &format!("deep{index}"));
            decisions.push(Rc::clone(&test));
            // Each test is fresh, so this direct arrangement is already normalized.
            value = Value {
                node: Rc::new(ValueNode::Select {
                    decision: test,
                    yes: Value::string(&format!("side{index}")),
                    no: value,
                }),
            };
        }
        let absent = decision(&arena, "absent");
        for taken in [true, false] {
            let specialized = value.under(&absent, taken);
            assert!(Rc::ptr_eq(&specialized.node, &value.node));
        }
        let all_false = decisions
            .iter()
            .rev()
            .map(|decision| Choice {
                decision,
                taken: false,
            })
            .fold(value, |value, choice| {
                value.under(choice.decision, choice.taken)
            });
        assert!(Rc::ptr_eq(&all_false.node, &terminal.node));
        assert!(all_false.constant().is_none());
    }

    #[test]
    fn specializing_one_branch_preserves_the_unaffected_sibling_identity() -> eyre::Result<()> {
        let arena = Arena::default();
        let outer = decision(&arena, "outer");
        let stable_test = decision(&arena, "stable");
        let selected = decision(&arena, "selected");
        let stable = Value::select(
            stable_test,
            Value::string("stable-yes"),
            Value::string("stable-no"),
        );
        let changed = Value::select(
            Rc::clone(&selected),
            Value::string("picked"),
            Value::string("other"),
        );
        let value = Value::select(Rc::clone(&outer), stable.clone(), changed);
        let specialized = value.under(&selected, true);
        let ValueNode::Select { decision, yes, no } = specialized.node.as_ref() else {
            eyre::bail!("independent outer decision was lost");
        };
        assert!(Rc::ptr_eq(decision, &outer));
        assert!(Rc::ptr_eq(&yes.node, &stable.node));
        sim_assert_eq!(have: no.constant(), want: Some(&GuardValue::string("picked")));
        Ok(())
    }

    #[test]
    fn mixed_specialization_preserves_same_decision_correlation_in_every_order() -> eyre::Result<()>
    {
        let arena = Arena::default();
        let selected = decision(&arena, "selected");
        let outer = decision(&arena, "outer");
        let inner = decision(&arena, "inner");
        let yes = Value::select(
            Rc::clone(&outer),
            Value::select(
                Rc::clone(&selected),
                Value::string("outer-selected"),
                Value::string("discarded-outer"),
            ),
            Value::select(
                Rc::clone(&inner),
                Value::select(
                    Rc::clone(&selected),
                    Value::string("inner-selected"),
                    Value::string("discarded-inner"),
                ),
                Value::string("fallback"),
            ),
        );
        let no = Value::select(
            Rc::clone(&outer),
            Value::select(
                Rc::clone(&selected),
                Value::string("discarded-negative"),
                Value::string("outer-negative"),
            ),
            Value::string("negative-fallback"),
        );
        let value = Value::select(Rc::clone(&selected), yes, no);
        for selected_taken in [true, false] {
            for outer_taken in [true, false] {
                for inner_taken in [true, false] {
                    let expected = if selected_taken {
                        if outer_taken {
                            "outer-selected"
                        } else if inner_taken {
                            "inner-selected"
                        } else {
                            "fallback"
                        }
                    } else if outer_taken {
                        "outer-negative"
                    } else {
                        "negative-fallback"
                    };
                    let forward = evaluate(
                        &value,
                        &[
                            Choice {
                                decision: &selected,
                                taken: selected_taken,
                            },
                            Choice {
                                decision: &outer,
                                taken: outer_taken,
                            },
                            Choice {
                                decision: &inner,
                                taken: inner_taken,
                            },
                        ],
                    )?;
                    let reordered = evaluate(
                        &value,
                        &[
                            Choice {
                                decision: &inner,
                                taken: inner_taken,
                            },
                            Choice {
                                decision: &selected,
                                taken: selected_taken,
                            },
                            Choice {
                                decision: &outer,
                                taken: outer_taken,
                            },
                        ],
                    )?;
                    sim_assert_eq!(have: forward, want: GuardValue::string(expected));
                    sim_assert_eq!(have: reordered, want: GuardValue::string(expected));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn specializing_equal_children_collapses_to_the_original_shared_scalar() {
        let arena = Arena::default();
        let selected = decision(&arena, "selected");
        let outer = decision(&arena, "outer");
        let shared = Value::string("same");
        let value = Value::select(
            outer,
            Value::select(
                Rc::clone(&selected),
                shared.clone(),
                Value::string("left-other"),
            ),
            Value::select(
                selected.clone(),
                shared.clone(),
                Value::string("right-other"),
            ),
        );
        let specialized = value.under(&selected, true);
        assert!(Rc::ptr_eq(&specialized.node, &shared.node));
    }
}

mod header_reconvergence_laws {
    use std::collections::BTreeSet;

    use helm_schema_ast::DefineIndex;
    use helm_schema_core::{Predicate, ValuesPath};
    use test_util::prelude::sim_assert_eq;

    use crate::analysis_db::IrAnalysisDb;
    use crate::value::{DocumentExecution, execute_document};

    fn run(source: &str) -> DocumentExecution {
        let mut defines = DefineIndex::new();
        defines.add_file_source(
            "templates/_helpers.tpl",
            r#"{{ define "marker" }}marked{{ end }}"#,
        );
        let db = IrAnalysisDb::new(&defines);
        execute_document(source, None, &db)
    }

    #[test]
    fn multi_exit_and_or_headers_do_not_execute_the_selected_body_twice() {
        let sources = [
            r#"{{ if or .Values.a .Values.b }}{{ include "marker" . }}{{ else }}other{{ end }}"#,
            r#"{{ if and .Values.a .Values.b }}other{{ else }}{{ include "marker" . }}{{ end }}"#,
        ];
        for source in sources {
            let execution = run(source);
            let body_invocations = execution
                .invocations()
                .filter(|invocation| invocation.function() == "include")
                .map(|invocation| invocation.invocation())
                .collect::<BTreeSet<_>>();
            sim_assert_eq!(have: body_invocations.len(), want: 1);
            assert!(execution.invocation_records_complete());
            sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
        }
    }

    #[test]
    fn header_reconvergence_preserves_conditional_writer_and_short_circuit_abort() {
        let written = run(
            r#"{{ $d := dict "x" "old" }}{{ if or .Values.choose (set $d "x" "new") }}seen{{ end }}{{ if eq (get $d "x") "old" }}{{ fail "old" }}{{ end }}"#,
        );
        sim_assert_eq!(have: written.abort_predicate(), want: Predicate::truthy_path("choose"));

        let aborted = run(
            r#"{{ if and .Values.stop (fail "stopped") }}{{ include "marker" . }}{{ end }}okay"#,
        );
        sim_assert_eq!(have: aborted.abort_predicate(), want: Predicate::truthy_path("stop"));
        assert!(
            aborted
                .invocations()
                .all(|invocation| invocation.function() != "include")
        );
        sim_assert_eq!(
            have: aborted.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(),
            want: vec![Some("okay".to_owned())]
        );
    }

    fn read_condition(execution: &DocumentExecution, path: &str) -> Predicate {
        let path = ValuesPath::parse(path);
        Predicate::Or(
            execution
                .reads()
                .iter()
                .filter(|read| read.values_path == path)
                .flat_map(|read| {
                    read.condition
                        .disjuncts()
                        .iter()
                        .map(|conjunction| Predicate::all(conjunction.iter().cloned().collect()))
                })
                .collect(),
        )
        .normalize_boolean()
    }

    fn assert_read_condition(execution: &DocumentExecution, path: &str, want: &Predicate) {
        let have = read_condition(execution, path);
        assert!(
            have.exactly_implies(want),
            "unexpected read of {path}: {have:?}; expected {want:?}"
        );
        assert!(
            want.exactly_implies(&have),
            "missing read of {path}: {have:?}; expected {want:?}"
        );
    }

    #[test]
    fn chained_else_if_preserves_body_and_short_circuited_operand_read_paths() {
        let execution = run(
            "{{ if or .Values.a .Values.b }}{{ .Values.first }}{{ else if and .Values.c .Values.d }}{{ .Values.second }}{{ else }}{{ .Values.third }}{{ end }}",
        );
        let first = Predicate::Or(vec![
            Predicate::truthy_path("a"),
            Predicate::truthy_path("b"),
        ]);
        let second = Predicate::all(vec![
            Predicate::truthy_path("c"),
            Predicate::truthy_path("d"),
        ]);
        assert_read_condition(&execution, "first", &first);
        assert_read_condition(
            &execution,
            "second",
            &Predicate::all(vec![first.negated(), second.clone()]),
        );
        assert_read_condition(
            &execution,
            "third",
            &Predicate::all(vec![first.negated(), second.negated()]),
        );
        assert_read_condition(&execution, "a", &Predicate::True);
        assert_read_condition(&execution, "b", &Predicate::truthy_path("a").negated());
        assert_read_condition(&execution, "c", &first.negated());
        assert_read_condition(
            &execution,
            "d",
            &Predicate::all(vec![first.negated(), Predicate::truthy_path("c")]),
        );
    }
}
