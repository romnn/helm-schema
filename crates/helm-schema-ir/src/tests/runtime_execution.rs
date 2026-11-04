use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::rc::Rc;

use crate::analysis_db::{IrAnalysisDb, ParsedTemplateSource};
use crate::value::DocumentExecution;
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

#[path = "runtime_constructors.rs"]
mod constructor_laws;
#[path = "runtime_helm_hosts.rs"]
mod host_laws;
#[path = "runtime_ranges.rs"]
mod range_laws;
#[path = "runtime_reconvergence.rs"]
mod reconvergence_laws;
#[path = "runtime_serialized_payload.rs"]
mod serialized_payload_laws;
#[path = "template_action_laws.rs"]
mod template_action_laws;

impl super::Heap {
    /// A session heap with a private predicate memo, for tests that build heaps directly.
    pub(super) fn new() -> Self {
        Self::with_predicate_memo(std::rc::Rc::default())
    }
}

impl super::HeapVersion {
    pub(super) fn object_ids(&self) -> Option<BTreeSet<super::ObjectId>> {
        self.query_object_ids(&mut super::heap_query::QueryBudget::new(65_536))
            .map(|objects| objects.into_iter().collect())
    }
}

impl super::State {
    /// A session's first state over a private heap, for tests that build states directly.
    pub(super) fn new(argument: super::Value) -> Self {
        Self::with_heap(argument, super::Heap::new())
    }
}

fn texts(source: &str) -> Vec<Option<String>> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    execute_document(source, Some("templates/test.yaml"), &db)
        .text()
        .map(|(_, text)| text.exact_text())
        .collect()
}

#[tracing::instrument(skip_all)]
pub(crate) fn execute_document(
    source: &str,
    source_path: Option<&str>,
    db: &IrAnalysisDb,
) -> DocumentExecution {
    let mut document = DocumentExecution::new(Vec::new(), super::FileOutput::Manifest);
    super::session::execute_files(
        vec![db.parsed_source(source, source_path, source_path.unwrap_or("gotpl"))],
        db,
        |_, execution| document = DocumentExecution::new(execution.exits, execution.output),
    );
    document
}

fn prepared_sources(
    sources: &[crate::symbolic::SymbolicTemplate<'_>],
    db: &IrAnalysisDb,
) -> Vec<Rc<ParsedTemplateSource>> {
    sources
        .iter()
        .map(|source| db.parsed_source(source.source, Some(source.source_path), source.source_path))
        .collect()
}

pub(crate) fn execute_documents(
    sources: &[crate::symbolic::SymbolicTemplate<'_>],
    db: &IrAnalysisDb,
) -> Vec<DocumentExecution> {
    let mut documents: Vec<_> = sources
        .iter()
        .map(|_| DocumentExecution::new(Vec::new(), crate::value::FileOutput::Manifest))
        .collect();
    super::session::execute_files(prepared_sources(sources, db), db, |index, execution| {
        if let Some(document) = documents.get_mut(index) {
            *document = DocumentExecution::new(execution.exits, execution.output);
        }
    });
    documents
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
fn partial_guards_with_surviving_tails_do_not_multiply_scheduled_files() {
    use crate::symbolic::SymbolicTemplate;
    use crate::value::{Flow, execute_documents};

    let db = IrAnalysisDb::new(&DefineIndex::new());
    for count in [1, 2, 4, 8] {
        let paths: Vec<_> = (0..count)
            .map(|index| format!("chart/templates/{index:02}.yaml"))
            .collect();
        let sources: Vec<_> = paths
            .iter()
            .map(|path| SymbolicTemplate {
                source: r#"{{ if eq (randAlphaNum 1) "a" }}{{ if .Values.bad }}{{ fail "bad" }}{{ end }}yes{{ else }}no{{ end }}"#,
                source_path: path,
            })
            .collect();
        let documents = execute_documents(&sources, &db);
        for document in documents {
            sim_assert_eq!(have: document.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
            sim_assert_eq!(have: document.abort_predicate(), want: Predicate::False);
        }
    }
}

#[test]
fn independent_optional_maps_reconverge_before_the_next_file() {
    use crate::symbolic::SymbolicTemplate;
    use crate::value::{Flow, execute_documents};

    for count in [1, 2, 4, 8] {
        let owned: Vec<_> = (0..count)
            .map(|index| {
                (
                    format!(r#"{{{{ if .Values.g{index} }}}}{{{{ $_ := set .Values.m{index} "v" "written" }}}}{{{{ end }}}}"#),
                    format!("chart/templates/{index:02}.yaml"),
                )
            })
            .collect();
        let sources: Vec<_> = owned
            .iter()
            .map(|(source, source_path)| SymbolicTemplate {
                source,
                source_path,
            })
            .collect();
        let db = IrAnalysisDb::new(&DefineIndex::new());
        for document in execute_documents(&sources, &db) {
            sim_assert_eq!(have: document.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
        }
    }
}

#[test]
fn frozen_scalars_do_not_retain_unrelated_heap_versions() -> eyre::Result<()> {
    use super::{Field, Heap, Remainder, Value};
    use std::rc::Rc;

    let mut heap = Heap::new();
    let map = heap.map(Remainder::Closed);
    let id = heap
        .version
        .object_ids()
        .expect("bounded test object roster")
        .into_iter()
        .next()
        .ok_or_eyre("map allocation missing")?;
    heap.write(&id, "x", Field::Present(Value::string("before")));
    let old_version = Rc::downgrade(&heap.version);
    let scalar = heap.freeze(&Value::string("independent"));
    heap.write(&id, "x", Field::Present(Value::string("after")));
    sim_assert_eq!(have: scalar.exact_text(), want: Some("independent".to_owned()));
    let frozen_map = heap.freeze(&map);
    heap.write(&id, "x", Field::Present(Value::string("later")));
    let field = frozen_map
        .read_field("x")
        .present()
        .ok_or_eyre("frozen map field missing")?;
    sim_assert_eq!(have: field.exact_text(), want: Some("after".to_owned()));
    drop(field);
    drop(frozen_map);
    drop(heap);
    assert!(old_version.upgrade().is_none());
    Ok(())
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
    sim_assert_eq!(have: heap.version.object_ids().expect("bounded test object roster").len(), want: 4097);
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
fn invalidation_shares_inputs_and_preserves_frozen_objects() -> eyre::Result<()> {
    use super::{Field, ObjectData, Remainder, State, Value};
    use std::rc::Rc;

    let mut state = State::new(Value::nil());
    let saved = state.heap.map(Remainder::Closed);
    let saved_id = state.map_target(&saved).ok_or_eyre("saved map missing")?;
    state
        .heap
        .write(&saved_id, "saved", Field::Present(Value::string("old")));
    let frozen = state.heap.freeze(&saved);
    for ordinal in 0..256 {
        let map = state.heap.map(Remainder::Closed);
        let id = state.map_target(&map).ok_or_eyre("map missing")?;
        state.heap.write(
            &id,
            "input",
            Field::Present(Value::input(helm_schema_core::ValuesPath::parse(&format!(
                "input{ordinal}"
            )))),
        );
    }

    super::operations::invalidate(&mut state);
    let object = state
        .heap
        .version
        .object(&saved_id)
        .ok_or_eyre("saved object missing")?;
    let ObjectData::Widened { inputs } = &object.data else {
        eyre::bail!("invalidation retained an exact object");
    };
    let shared_inputs = Rc::clone(inputs);
    sim_assert_eq!(have: shared_inputs.len(), want: 256);
    for object in state
        .heap
        .version
        .object_ids()
        .expect("bounded test object roster")
        .into_iter()
        .filter_map(|id| state.heap.version.object(&id))
    {
        let ObjectData::Widened { inputs } = &object.data else {
            eyre::bail!("invalidation retained an exact object");
        };
        assert!(Rc::ptr_eq(&shared_inputs, inputs));
    }

    // Extending one object must leave both its peers and older observations unchanged.
    state.heap.write(
        &saved_id,
        "later",
        Field::Present(Value::input(helm_schema_core::ValuesPath::parse("later"))),
    );
    sim_assert_eq!(have: shared_inputs.len(), want: 256);
    let current = state
        .heap
        .version
        .object(&saved_id)
        .ok_or_eyre("written object missing")?;
    let ObjectData::Map {
        fields,
        remainder: Remainder::Unknown {
            inputs: current_inputs,
        },
    } = &current.data
    else {
        eyre::bail!("write lost the unknown remainder");
    };
    let later = helm_schema_core::ValuesPath::parse("later");
    sim_assert_eq!(have: fields.get("later"), want: Some(&Field::Present(Value::input(later.clone()))));
    assert!(Rc::ptr_eq(&shared_inputs, current_inputs));
    let inputs = state.heap.freeze(&saved).inputs();
    sim_assert_eq!(have: inputs.len(), want: 257);
    assert!(inputs.contains(&later));
    sim_assert_eq!(have: state.heap.version.field(&saved_id, "unknown"), want: Field::Unknown);
    for (id, object) in state
        .heap
        .version
        .object_ids()
        .expect("bounded test object roster")
        .into_iter()
        .filter_map(|id| state.heap.version.object(&id).map(|storage| (id, storage)))
    {
        if id == saved_id {
            continue;
        }
        let ObjectData::Widened { inputs } = &object.data else {
            eyre::bail!("peer recovered an exact object");
        };
        assert!(Rc::ptr_eq(&shared_inputs, inputs));
    }
    let old = frozen
        .read_field("saved")
        .present()
        .ok_or_eyre("saved field disappeared")?;
    sim_assert_eq!(have: old.literal(), want: Some(&helm_schema_core::GuardValue::String("old".to_owned())));
    Ok(())
}

#[test]
fn consuming_immutable_alternatives_preserves_their_shared_nodes() {
    use super::{Coverage, Heap, Value};
    use std::rc::Rc;

    let heap = Heap::new();
    let predicate = Predicate::truthy_path("flag");
    let decision = heap
        .arena
        .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let selected = Value::select(decision, Value::string("yes"), Value::string("no"));
    let consumed = heap.version.consume(&selected);
    assert!(Rc::ptr_eq(&selected.node, &consumed.node));
}

#[test]
fn joining_widened_objects_shares_the_union_of_branch_inputs() -> eyre::Result<()> {
    use super::{Coverage, Heap, Multiplicity, ObjectData, ObjectVersion, Remainder};
    use std::collections::BTreeSet;
    use std::rc::Rc;

    let mut heap = Heap::new();
    let ids: Vec<_> = (0..256)
        .map(|_| {
            heap.allocate(
                ObjectData::Map {
                    fields: Rc::new(std::collections::BTreeMap::default()),
                    remainder: Remainder::Closed,
                },
                Multiplicity::Singleton,
            )
        })
        .collect();
    let mut yes = heap.fork();
    let mut no = heap.fork();
    let yes_inputs = Rc::new(BTreeSet::from([helm_schema_core::ValuesPath::parse("yes")]));
    let no_inputs = Rc::new(BTreeSet::from([helm_schema_core::ValuesPath::parse("no")]));
    for id in &ids {
        yes.install(
            id.clone(),
            ObjectVersion {
                revision: heap.arena.snapshot(),
                multiplicity: Multiplicity::Singleton,
                data: ObjectData::Widened {
                    inputs: Rc::clone(&yes_inputs),
                },
            },
        );
        no.install(
            id.clone(),
            ObjectVersion {
                revision: heap.arena.snapshot(),
                multiplicity: Multiplicity::Singleton,
                data: ObjectData::Widened {
                    inputs: Rc::clone(&no_inputs),
                },
            },
        );
    }
    let predicate = Predicate::truthy_path("flag");
    let decision = heap
        .arena
        .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    heap.join(&decision, &yes, &no)?;
    let mut shared = None;
    for id in ids {
        let object = heap
            .version
            .object(&id)
            .ok_or_eyre("joined object missing")?;
        let ObjectData::Widened { inputs } = &object.data else {
            eyre::bail!("join recovered an exact object");
        };
        sim_assert_eq!(have: inputs.as_ref(), want: &yes_inputs.union(&no_inputs).cloned().collect());
        if let Some(previous) = &shared {
            assert!(Rc::ptr_eq(previous, inputs));
        } else {
            shared = Some(Rc::clone(inputs));
        }
    }
    sim_assert_eq!(have: yes_inputs.len(), want: 1);
    sim_assert_eq!(have: no_inputs.len(), want: 1);
    Ok(())
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
    sim_assert_eq!(have: super::DocumentExecution::new(exits, crate::value::FileOutput::Manifest).abort_predicate(), want: Predicate::False);
    Ok(())
}

#[test]
fn reconverged_span_does_not_drop_the_missing_conditional_suffix() {
    let source = "prefix{{ if .Values.flag }}suffix{{ end }}";
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, None, &db);
    let outcomes = execution
        .executed_outputs()
        .into_iter()
        .map(|output| (output.condition, output.text))
        .collect::<Vec<_>>();

    // Both complete strings retain their executed guard; no unconditional concatenation is valid.
    let flag = Predicate::truthy_path("flag");
    sim_assert_eq!(
        have: outcomes.into_iter().collect::<BTreeSet<_>>(),
        want: BTreeSet::from([
            (flag.clone(), Some("prefixsuffix".to_owned())),
            (flag.negated().normalize_boolean(), Some("prefix".to_owned())),
        ])
    );
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

    // Each API version is complete and belongs only to its executed control arm.
    let legacy = Predicate::truthy_path("legacy");
    sim_assert_eq!(
        have: execution.placed_documents().flat_map(|document| {
            crate::resource_identity::placed_resources(&document, &execution)
                .into_iter()
                .map(|resource| (resource.condition, Some(resource.resource.api_version)))
        }).collect::<BTreeSet<_>>(),
        want: BTreeSet::from([
            (legacy.clone(), Some("apps/v1beta1".to_owned())),
            (legacy.negated().normalize_boolean(), Some("apps/v1".to_owned())),
        ])
    );
    let body = crate::resource_identity::test_support::executed_scalar_body(&execution, span, None);
    sim_assert_eq!(have: body.all_literals(), want: vec!["apps/v1beta1".to_owned(), "apps/v1".to_owned()]);
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
        crate::value::tests::execute_document(source, None, &db)
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
        execute(&source).observed_marker_condition(span)
    }

    #[test]
    fn joined_set_presence_keeps_writer_condition() {
        let raw_present = Predicate::from(Guard::HasKey {
            path: ValuesPath::default(),
            key: "x".to_owned(),
        });
        sim_assert_eq!(
            have: present_condition(r#"{{ if .Values.choose }}{{ $_ := set .Values "x" "new" }}{{ end }}"#),
            want: Predicate::Or(vec![Predicate::truthy_path("choose"), raw_present]).normalize_boolean()
        );
    }

    #[test]
    fn joined_unset_presence_keeps_writer_condition() {
        let raw_present = Predicate::from(Guard::HasKey {
            path: ValuesPath::default(),
            key: "x".to_owned(),
        });
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
fn sixty_four_independent_controls_reconverge_to_one_normal_state() -> eyre::Result<()> {
    let mut source = String::new();
    for index in 0..64 {
        write!(
            source,
            "{{{{ if .Values.g{index} }}}}item{index}: fixed\n{{{{ end }}}}\n"
        )?;
    }
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(&source, None, &db);
    sim_assert_eq!(have: execution.exits.len(), want: 1);
    assert!(execution.exits.iter().all(|exit| matches!(exit.flow, super::Flow::Normal(_)) && exit.state.active.is_empty()));
    sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
    Ok(())
}

#[test]
fn conditional_parameter_failures_reconverge_surviving_states() -> eyre::Result<()> {
    for count in [1, 2, 8, 32] {
        let mut source = String::new();
        for index in 0..count {
            write!(
                source,
                "{{{{ if .Values.g{index} }}}}{{{{ $_ := lower .Values.s{index} }}}}{{{{ end }}}}"
            )?;
        }
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(&source, None, &db);
        let normal = execution
            .exits
            .iter()
            .filter(|exit| matches!(exit.flow, super::Flow::Normal(_)))
            .collect::<Vec<_>>();
        sim_assert_eq!(have: normal.len(), want: 1);
        sim_assert_eq!(have: execution.exits.len(), want: count + 1);
        let failed: Vec<_> = (0..count)
            .map(|index| {
                Predicate::all(vec![
                    Predicate::truthy_path(format!("g{index}")),
                    Predicate::from(helm_schema_core::Guard::TypeIs {
                        path: helm_schema_core::ValuesPath::parse(&format!("s{index}")),
                        schema_type: "string".to_owned(),
                    })
                    .negated(),
                ])
            })
            .collect();
        let success = Predicate::all(
            failed
                .iter()
                .map(|failure| failure.negated().normalize_boolean())
                .collect(),
        )
        .normalize_boolean();
        sim_assert_eq!(
            have: normal.first().map(|exit| exit.state.path_predicate().normalize_boolean()),
            want: Some(success),
        );
        if count <= 2 {
            let expected = Predicate::Or(failed);
            let observed = execution.abort_predicate();
            assert!(observed.exactly_implies(&expected));
            assert!(expected.exactly_implies(&observed));
        }
    }
    Ok(())
}

#[test]
fn repeated_conditional_consumption_reuses_the_surviving_condition() {
    let source = r"{{ if .Values.enabled }}{{ $_ := lower .Values.input }}{{ end }}".repeat(32);
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(&source, None, &db);
    let normal = execution
        .exits
        .iter()
        .filter(|exit| matches!(exit.flow, super::Flow::Normal(_)))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: normal.len(), want: 1);
    sim_assert_eq!(have: normal.first().map(|exit| exit.state.active.len()), want: Some(1));
}

#[test]
fn allocation_budget_counts_classes_in_the_current_heap_version() -> eyre::Result<()> {
    let mut heap = super::Heap::new();
    for _ in 0..4096 {
        let mut branch = heap.fork();
        branch.map(super::Remainder::Closed);
    }
    let value = heap.map(super::Remainder::Closed);
    let super::ValueNode::Leaf(super::Leaf {
        datum: super::Datum::Map {
            target: Some(id),
            view: _,
        },
        origin: _,
    }) = value.node.as_ref()
    else {
        eyre::bail!("map allocation lost its object target");
    };
    let object = heap
        .version
        .object(id)
        .ok_or_eyre("allocated object missing")?;
    sim_assert_eq!(have: object.multiplicity, want: super::Multiplicity::Singleton);
    sim_assert_eq!(have: heap.version.object_ids().expect("bounded test object roster").len(), want: 1);
    assert!(heap.arena.aggregate_summary.get().is_none());
    Ok(())
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
fn nested_one_sided_input_materialization_preserves_scalar_type_alternatives() {
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
    let expected = Predicate::Or(vec![
        Predicate::truthy_path("flag"),
        Predicate::from(helm_schema_core::Guard::TypeIs {
            path: helm_schema_core::ValuesPath::parse("obj"),
            schema_type: "string".to_owned(),
        })
        .negated(),
    ]);
    runtime_guard_json_domain::equal_on_models(
        &execution.abort_predicate(),
        &expected,
        &runtime_guard_json_domain::single_input_models(),
    );
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
                inputs: Rc::new(BTreeSet::from([helm_schema_core::ValuesPath::parse(
                    "lost",
                )])),
            },
        },
    );
    let joined = State::join_branches(
        &yes.heap.arena.join_paths(&yes.active, &no.active),
        &yes,
        &no,
    )
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
    use crate::value::DocumentExecution;
    use crate::value::tests::execute_document;

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
                .map(super::super::ExecutedInvocationRecord::invocation)
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

mod render_session {
    use crate::analysis_db::IrAnalysisDb;
    use crate::symbolic::SymbolicTemplate;
    use crate::value::execute_documents;
    use helm_schema_ast::DefineIndex;
    use helm_schema_core::Predicate;
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn reverse_lexical_file_order_preserves_the_writer_before_the_reader() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let sources = [
            SymbolicTemplate {
                source: r#"{{ if .Values.enabled }}{{ $_ := required "bad" }}{{ end }}okay"#,
                source_path: "chart/templates/a-check.yaml",
            },
            SymbolicTemplate {
                source: r#"{{ $_ := set .Values "enabled" false }}writer"#,
                source_path: "chart/templates/z-writer.yaml",
            },
        ];
        let execution = execute_documents(&sources, &db);
        sim_assert_eq!(
            have: execution.iter().map(super::super::DocumentExecution::abort_predicate).collect::<Vec<_>>(),
            want: vec![Predicate::False, Predicate::False]
        );
        sim_assert_eq!(
            have: execution.iter().map(|document| document.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>()).collect::<Vec<_>>(),
            want: vec![vec![Some("okay".to_owned())], vec![Some("writer".to_owned())]]
        );
        // The second top-level program still publishes its own sites.
        sim_assert_eq!(
            have: execution.iter().map(|document| document.text_sites().map(|site| site.bytes.clone()).collect::<Vec<_>>()).collect::<Vec<_>>(),
            want: vec![vec!["okay".to_owned()], vec!["writer".to_owned()]]
        );
    }

    #[test]
    fn every_reaching_alternative_retains_its_top_level_program_sites() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let sources = [
            SymbolicTemplate {
                source: "later",
                source_path: "chart/templates/a-read.yaml",
            },
            SymbolicTemplate {
                source: r#"{{ if .Values.choose }}{{ $_ := set .Values.nested "x" "yes" }}{{ end }}"#,
                source_path: "chart/templates/z-write.yaml",
            },
        ];
        let execution = execute_documents(&sources, &db);
        let later: Vec<_> = execution
            .iter()
            .flat_map(super::super::DocumentExecution::text_sites)
            .filter(|site| site.bytes == "later")
            .collect();
        sim_assert_eq!(have: later.len(), want: 1);
        let observed =
            Predicate::Or(later.iter().map(|site| site.condition()).collect()).normalize_boolean();
        let completed = Predicate::Or(
            execution
                .iter()
                .flat_map(super::super::DocumentExecution::text)
                .filter(|(_, text)| text.exact_text().as_deref() == Some("later"))
                .map(|(condition, _)| condition)
                .collect(),
        );
        // Only normally completed alternatives reach this file; invalid set arguments may abort.
        assert!(observed.exactly_implies(&completed));
        assert!(completed.exactly_implies(&observed));
    }

    #[test]
    fn deeper_files_execute_before_shallower_files() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let sources = [
            SymbolicTemplate {
                source: r"{{ .Values.message }}",
                source_path: "chart/templates/z-read.yaml",
            },
            SymbolicTemplate {
                source: r#"{{ $_ := set .Values "message" "deep" }}"#,
                source_path: "chart/templates/nested/a-write.yaml",
            },
        ];
        let execution = execute_documents(&sources, &db);
        sim_assert_eq!(
            have: execution.iter().flat_map(|document| document.text().map(|(_, text)| text.exact_text())).collect::<Vec<_>>(),
            want: vec![Some("deep".to_owned()), Some(String::new())]
        );
    }

    #[test]
    fn abort_stops_later_files_and_a_new_session_has_a_fresh_heap() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let sources = [
            SymbolicTemplate {
                source: r"{{ .Values.unreachable }}",
                source_path: "chart/templates/a-later.yaml",
            },
            SymbolicTemplate {
                source: r#"{{ fail "stopped" }}"#,
                source_path: "chart/templates/z-stop.yaml",
            },
        ];
        let execution = execute_documents(&sources, &db);
        sim_assert_eq!(have: execution.iter().map(super::super::DocumentExecution::abort_predicate).collect::<Vec<_>>(), want: vec![Predicate::False, Predicate::True]);
        sim_assert_eq!(have: execution.iter().map(|document| document.reads().len()).collect::<Vec<_>>(), want: vec![0, 0]);
        let fresh = execute_documents(&sources[..1], &db);
        sim_assert_eq!(have: fresh.iter().flat_map(|document| document.text().map(|(_, text)| text.exact_text())).collect::<Vec<_>>(), want: vec![None]);
    }

    #[test]
    fn template_metadata_is_fresh_but_prior_references_remain_frozen() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let sources = [
            SymbolicTemplate {
                source: r"{{ .Values.saved.Name }}|{{ .Template.Name }}|{{ .Template.BasePath }}",
                source_path: "chart/templates/a-read.yaml",
            },
            SymbolicTemplate {
                source: r#"{{ $local := "file-local" }}{{ $_ := set .Values "saved" .Template }}"#,
                source_path: "chart/templates/z-write.yaml",
            },
        ];
        let execution = execute_documents(&sources, &db);
        sim_assert_eq!(
            have: execution.iter().flat_map(|document| document.text().map(|(_, text)| text.exact_text())).collect::<Vec<_>>(),
            want: vec![Some("chart/templates/z-write.yaml|chart/templates/a-read.yaml|chart/templates".to_owned()), Some(String::new())]
        );
        sim_assert_eq!(
            have: execution.iter().map(|document| document.exits.iter().all(|exit| exit.state.read_local("local").is_none())).collect::<Vec<_>>(),
            want: vec![true, false]
        );
    }
}

mod render_stripping {
    use crate::analysis_db::IrAnalysisDb;
    use crate::value::tests::execute_document;
    use helm_schema_ast::DefineIndex;
    use helm_schema_core::Predicate;
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn file_stripping_crosses_literal_and_emission_boundaries() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        for source in [
            "A<no value>B",
            r#"A<no {{ "value" }}>B"#,
            r#"A{{ "<no " }}value>B"#,
            r#"A{{ "<no " }}{{ "value>" }}B"#,
        ] {
            let execution = execute_document(source, Some("chart/templates/test.yaml"), &db);
            sim_assert_eq!(
                have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(),
                want: vec![Some("AB".to_owned())],
                "{source}"
            );
        }
    }

    #[test]
    fn include_preserves_its_marker_until_the_file_boundary() {
        let mut defines = DefineIndex::new();
        defines.add_file_source(
            "chart/templates/_helpers.tpl",
            r#"{{ define "marker" }}<no value>{{ end }}{{ define "prefix" }}<no {{ end }}"#,
        );
        let db = IrAnalysisDb::new(&defines);
        let execution = execute_document(
            r#"{{ if ne (include "marker" .) "<no value>" }}{{ fail "include stripped early" }}{{ end }}A{{ include "prefix" . }}value>B"#,
            Some("chart/templates/test.yaml"),
            &db,
        );
        sim_assert_eq!(have: execution.abort_predicate(), want: Predicate::False);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("AB".to_owned())]);
    }

    #[test]
    fn stripping_demotes_only_the_overlapping_rendered_sites() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(
            r#"A<no {{ "value" }}>B{{ "untouched" }}"#,
            Some("chart/templates/test.yaml"),
            &db,
        );
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("ABuntouched".to_owned())]);
        sim_assert_eq!(have: execution.sites().map(super::super::ExecutedSite::coverage_exact).collect::<Vec<_>>(), want: vec![true, true]);
        sim_assert_eq!(have: execution.sites().map(|site| site.rendered_unchanged).collect::<Vec<_>>(), want: vec![false, true]);
        sim_assert_eq!(have: execution.text_sites().map(|site| site.coverage_exact).collect::<Vec<_>>(), want: vec![true, true]);
        sim_assert_eq!(have: execution.text_sites().map(|site| site.rendered_unchanged).collect::<Vec<_>>(), want: vec![false, false]);
    }

    #[test]
    fn unknown_output_does_not_demote_unaffected_earlier_text_or_guards() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(
            r"{{ if true }}earlier{{ end }}{{ .Values.unknown }}later",
            Some("chart/templates/test.yaml"),
            &db,
        );
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![None]);
        sim_assert_eq!(have: execution.sites().map(super::super::ExecutedSite::coverage_exact).collect::<Vec<_>>(), want: vec![true, true]);
        sim_assert_eq!(have: execution.sites().map(|site| site.rendered_unchanged).collect::<Vec<_>>(), want: vec![true, false]);
        sim_assert_eq!(have: execution.text_sites().map(|site| site.coverage_exact).collect::<Vec<_>>(), want: vec![true, true]);
    }
}

mod observation_certainty {
    use crate::analysis_db::IrAnalysisDb;
    use crate::value::ScalarValueDispatch;
    use crate::value::tests::execute_document;
    use color_eyre::eyre::{self, OptionExt as _};
    use helm_schema_ast::DefineIndex;
    use helm_schema_core::{GuardValue, ValuesPath};
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn later_output_cap_does_not_rewrite_an_earlier_exact_observation() -> eyre::Result<()> {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let source = format!("{{{{ \"earlier\" }}}}{}", r#"{{ "x" }}"#.repeat(129));
        let execution = execute_document(&source, None, &db);
        let earlier = execution
            .sites()
            .find(|site| site.value().literal() == Some(&GuardValue::string("earlier")))
            .ok_or_eyre("missing earlier observation")?;
        assert!(earlier.coverage_exact());
        sim_assert_eq!(
            have: earlier.projected_scalar(),
            want: Some(ScalarValueDispatch::bind_content(vec![(helm_schema_core::Predicate::True, super::super::ScalarContent::Literal(GuardValue::string("earlier")))], true))
        );
        sim_assert_eq!(have: earlier.rendered_scalar(), want: earlier.projected_scalar());
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![None]);
        Ok(())
    }

    #[test]
    fn later_partial_control_does_not_rewrite_an_earlier_exact_observation() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(
            r#"{{ if true }}{{ "earlier" }}{{ end }}{{ if eq (randAlphaNum 1) "a" }}left{{ else }}right{{ end }}"#,
            None,
            &db,
        );
        let earlier: Vec<_> = execution
            .sites()
            .filter(|site| site.value().literal() == Some(&GuardValue::string("earlier")))
            .collect();
        assert!(!earlier.is_empty());
        assert!(earlier.iter().all(|site| site.coverage_exact()));
        assert!(earlier.iter().all(|site| site.projected_scalar().is_some()));
        assert!(earlier.iter().all(|site| site.rendered_scalar().is_some()));
    }

    #[test]
    fn raw_expression_identity_survives_with_only_marker_safe_placement() -> eyre::Result<()> {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document("number: {{ .Values.number }}", None, &db);
        let output = execution
            .sites()
            .find(|site| site.rendered())
            .ok_or_eyre("missing raw output observation")?;
        assert!(output.coverage_exact());
        sim_assert_eq!(have: output.value().identity(), want: Some(&ValuesPath::parse("number")));
        sim_assert_eq!(
            have: output.projected_scalar(),
            want: Some(ScalarValueDispatch::bind_content(vec![(helm_schema_core::Predicate::True, super::super::ScalarContent::Identity(ValuesPath::parse("number")))], true))
        );
        // Marker-bearing strings retain no placement claim, while safe scalar inputs do.
        let rendered = output
            .rendered_scalar()
            .ok_or_eyre("missing safe scalar projection")?;
        let content = rendered
            .content()
            .ok_or_eyre("missing safe scalar content")?;
        sim_assert_eq!(have: content.arms().count(), want: 1);
        assert!(
            content
                .arms()
                .all(|(condition, _)| condition != &helm_schema_core::Predicate::True)
        );
        Ok(())
    }

    #[test]
    fn removed_marker_does_not_rewrite_the_expression_snapshot_or_claim_placement_identity()
    -> eyre::Result<()> {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(r#"number: {{ "1<no value>" }}"#, None, &db);
        let output = execution
            .sites()
            .find(|site| site.rendered())
            .ok_or_eyre("missing marker output observation")?;
        sim_assert_eq!(have: output.value().literal(), want: Some(&GuardValue::string("1<no value>")));
        assert!(output.coverage_exact());
        sim_assert_eq!(
            have: output.projected_scalar(),
            want: Some(ScalarValueDispatch::bind_content(vec![(helm_schema_core::Predicate::True, super::super::ScalarContent::Literal(GuardValue::string("1<no value>")))], true))
        );
        sim_assert_eq!(have: output.rendered_scalar(), want: None);
        sim_assert_eq!(have: execution.text().map(|(_, text)| text.exact_text()).collect::<Vec<_>>(), want: vec![Some("number: 1".to_owned())]);
        Ok(())
    }
}

mod input_validation_reuse {
    use super::super::kinds::{GoType, validate_parameter};
    use super::super::{ArgumentForm, Coverage, Exit, Flow, State, Value, ValueNode};
    use color_eyre::eyre::{self, OptionExt as _};
    use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
    use std::collections::BTreeSet;
    use test_util::prelude::sim_assert_eq;

    fn normal_exit(exits: Vec<Exit>) -> eyre::Result<Exit> {
        exits
            .into_iter()
            .find(|exit| matches!(exit.flow, Flow::Normal(_)))
            .ok_or_eyre("normal validation exit missing")
    }

    #[test]
    fn repeated_input_validation_keeps_one_accepted_edge() -> eyre::Result<()> {
        let input = Value::input(ValuesPath::parse("x"));
        let mut state = State::new(Value::nil());
        for _ in 0..128 {
            let exit = normal_exit(validate_parameter(
                state,
                input.clone(),
                Some(GoType::String),
                ArgumentForm::Evaluated,
            ))?;
            sim_assert_eq!(have: exit.state.active.len(), want: 1);
            let Flow::Normal(normal) = &exit.flow else {
                eyre::bail!("validation did not return normally");
            };
            sim_assert_eq!(have: exit.state.heap.freeze(&normal.value).go_type(), want: GoType::String);
            state = exit.state;
        }
        Ok(())
    }

    #[test]
    fn known_rejection_does_not_add_a_redundant_edge_and_partial_edges_do_not_refine() {
        let input = Value::input(ValuesPath::parse("x"));
        let state = State::new(Value::nil());
        let accepted = Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("x"),
            schema_type: "string".to_owned(),
        });
        let exact =
            state
                .heap
                .arena
                .decision(accepted.clone(), accepted.negated(), Coverage::Exact);
        let rejected = state.fork(exact, false);
        let exits = validate_parameter(
            rejected,
            input.clone(),
            Some(GoType::String),
            ArgumentForm::Evaluated,
        );
        sim_assert_eq!(have: exits.len(), want: 1);
        assert!(
            exits
                .iter()
                .all(|exit| matches!(exit.flow, Flow::Abort(_)) && exit.state.active.len() == 1)
        );

        let partial =
            state
                .heap
                .arena
                .decision(accepted.clone(), accepted.negated(), Coverage::Partial);
        let unresolved = state.fork(partial, true);
        let exits = validate_parameter(
            unresolved,
            input,
            Some(GoType::String),
            ArgumentForm::Evaluated,
        );
        sim_assert_eq!(have: exits.len(), want: 2);
        assert!(exits.iter().all(|exit| exit.state.active.len() == 2));
    }

    #[test]
    fn repeated_container_validation_preserves_the_typed_nil_split() -> eyre::Result<()> {
        let input = Value::input(ValuesPath::parse("x"));
        let first = normal_exit(validate_parameter(
            State::new(Value::nil()),
            input.clone(),
            Some(GoType::Map),
            ArgumentForm::Evaluated,
        ))?;
        let second = normal_exit(validate_parameter(
            first.state,
            input,
            Some(GoType::Map),
            ArgumentForm::Evaluated,
        ))?;
        sim_assert_eq!(have: second.state.active.len(), want: 1);
        let Flow::Normal(normal) = second.flow else {
            eyre::bail!("validation did not return normally");
        };
        let ValueNode::Select {
            decision: _,
            yes,
            no,
        } = normal.value.node.as_ref()
        else {
            eyre::bail!("typed nil split disappeared");
        };
        sim_assert_eq!(have: second.state.heap.freeze(yes).go_type(), want: GoType::Map);
        sim_assert_eq!(have: second.state.heap.freeze(no).go_type(), want: GoType::Map);
        sim_assert_eq!(have: super::super::kinds::nilness(yes), want: Some(true));
        Ok(())
    }

    #[test]
    fn later_alias_writes_do_not_reuse_a_stale_field_origin() -> eyre::Result<()> {
        use super::super::{Field, Remainder};
        let mut state = State::new(Value::nil());
        let map = state.heap.map(Remainder::Input);
        let id = state.map_target(&map).ok_or_eyre("input map missing")?;
        state.heap.register_input(id.clone(), ValuesPath::parse(""));
        state.heap.arena.input_objects.borrow_mut().insert(
            ValuesPath::parse(""),
            crate::value::InputBacking {
                object: id.clone(),
                map_type: state.heap.arena.decision(
                    Predicate::True,
                    Predicate::False,
                    Coverage::Exact,
                ),
            },
        );
        let captured = state.field(&map, "x");
        let first = normal_exit(validate_parameter(
            state,
            captured.clone(),
            Some(GoType::String),
            ArgumentForm::Evaluated,
        ))?;
        let mut state = first.state;
        state
            .heap
            .write(&id, "x", Field::Present(Value::literal(GuardValue::Int(7))));
        let replaced = state.field(&map, "x");
        let exits = validate_parameter(
            state.clone(),
            replaced,
            Some(GoType::String),
            ArgumentForm::Evaluated,
        );
        assert!(exits.iter().all(|exit| matches!(exit.flow, Flow::Abort(_))));
        // The captured scalar still denotes the original input, even after replacing its field.
        let captured = normal_exit(validate_parameter(
            state.clone(),
            captured,
            Some(GoType::String),
            ArgumentForm::Evaluated,
        ))?;
        sim_assert_eq!(have: captured.state.active.len(), want: 1);
        state.heap.write(
            &id,
            "x",
            Field::Present(Value::influenced(BTreeSet::from([ValuesPath::parse("x")]))),
        );
        let replaced = state.field(&map, "x");
        assert!(replaced.identity().is_none());
        let unknown = normal_exit(validate_parameter(
            state,
            replaced,
            Some(GoType::String),
            ArgumentForm::Evaluated,
        ))?;
        let Flow::Normal(normal) = unknown.flow else {
            eyre::bail!("validation did not return normally");
        };
        sim_assert_eq!(have: unknown.state.heap.freeze(&normal.value).go_type(), want: GoType::Unknown);
        Ok(())
    }
}

#[path = "runtime_reads.rs"]
mod read_safety;

#[path = "runtime_read_projection.rs"]
mod read_projection_laws;

mod scheduled_file_occurrence_laws {
    use std::collections::BTreeSet;
    use std::rc::Rc;

    use crate::analysis_db::IrAnalysisDb;
    use crate::symbolic::SymbolicTemplate;
    use crate::value::{Coverage, Flow, FrameBoundary, Remainder, State, Value, execute_documents};
    use color_eyre::eyre::{self, OptionExt as _};
    use helm_schema_ast::DefineIndex;
    use helm_schema_core::{Guard, Predicate, ValuesPath};
    use indoc::indoc;
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn an_explicit_pair_of_incoming_partitions_uses_one_file_identity() {
        let mut state = State::new(Value::nil());
        let root = state.heap.map(Remainder::Closed);
        let predicate = Predicate::truthy_path("flag");
        let decision =
            state
                .heap
                .arena
                .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        let occurrence = crate::value::session::FileOccurrence::new(&state);
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let parsed = db.parsed_source(
            "# done",
            Some("chart/templates/a.yaml"),
            "chart/templates/a.yaml",
        );
        let mut exits = Vec::new();
        for taken in [true, false] {
            let mut branch = state.fork(Rc::clone(&decision), taken);
            branch.frames.clear();
            branch.enter_file_call(&root, &occurrence);
            exits.extend(crate::value::eval::Executor { db: &db }.parsed_file_body(
                "# done",
                Some("chart/templates/a.yaml"),
                parsed.root.syntax(),
                branch,
                &occurrence,
            ));
        }
        let joined = crate::value::control::reconverge(exits);
        sim_assert_eq!(have: joined.len(), want: 1);
        sim_assert_eq!(have: joined.first().map(|exit| exit.state.program), want: Some(occurrence.program));
    }

    #[test]
    fn compatible_incoming_partitions_share_one_scheduled_file_occurrence() -> eyre::Result<()> {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let sources = [
            SymbolicTemplate {
                source: r#"{{ $_ := set .Values.obj "unused" "written" }}"#,
                source_path: "chart/templates/a.yaml",
            },
            SymbolicTemplate {
                source: r#"{{ if .Values.flag }}{{ $_ := set .Values.obj "unused" "written" }}{{ end }}"#,
                source_path: "chart/templates/z.yaml",
            },
        ];
        let documents = execute_documents(&sources, &db);
        let first = documents
            .get(1)
            .ok_or_eyre("first scheduled file missing")?;
        let second = documents
            .first()
            .ok_or_eyre("second scheduled file missing")?;
        sim_assert_eq!(have: second.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
        let programs = second
            .exits
            .iter()
            .map(|exit| exit.state.program)
            .collect::<BTreeSet<_>>();
        sim_assert_eq!(have: programs.len(), want: 1);
        let first_programs = first
            .exits
            .iter()
            .map(|exit| exit.state.program)
            .collect::<BTreeSet<_>>();
        assert!(programs.is_disjoint(&first_programs));
        let calls = second
            .exits
            .iter()
            .filter_map(|exit| exit.state.frames.first())
            .filter_map(|frame| match frame.boundary {
                FrameBoundary::Call(call) => Some(call),
                FrameBoundary::Lexical => None,
            })
            .collect::<BTreeSet<_>>();
        sim_assert_eq!(have: calls.len(), want: 1);
        let expected = Predicate::Or(vec![
            Predicate::from(Guard::TypeIs {
                path: ValuesPath::parse("obj"),
                schema_type: "object".to_owned(),
            })
            .negated(),
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse("obj"),
                value: helm_schema_core::GuardValue::Null,
            }),
            Predicate::from(Guard::Absent {
                path: ValuesPath::parse("obj"),
            }),
        ]);
        let observed = Predicate::Or(
            documents
                .iter()
                .map(super::super::DocumentExecution::abort_predicate)
                .collect(),
        )
        .normalize_boolean();
        crate::value::tests::runtime_guard_json_domain::equal_on_models(
            &observed,
            &expected,
            &crate::value::tests::runtime_guard_json_domain::single_input_models(),
        );
        Ok(())
    }

    #[test]
    fn shared_file_occurrence_preserves_cross_file_aliases_template_snapshots_and_local_frames()
    -> eyre::Result<()> {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let sources = [
            SymbolicTemplate {
                source: indoc! {r#"
                    {{- $_ := set .Values.obj "unused" "written" -}}
                    {{- $captured := .Values.shared -}}
                    {{- $_ := set $captured "v" "new" -}}
                    {{- if ne .Values.shared.v "new" -}}{{ fail "shared alias lost" }}{{- end -}}
                    {{- if eq .Values.savedTemplate.Name .Template.Name -}}{{ fail "Template reused" }}{{- end }}
                    data: {{ .Values.shared.v | quote }}
                "#},
                source_path: "chart/templates/a.yaml",
            },
            SymbolicTemplate {
                source: indoc! {r#"
                    {{- $fileLocal := "previous file" -}}
                    {{- $box := dict "v" "old" -}}
                    {{- $_ := set .Values "shared" $box -}}
                    {{- $_ := set .Values "savedTemplate" .Template -}}
                    {{- if .Values.flag -}}{{ $_ := set .Values.obj "unused" "written" }}{{- end -}}
                "#},
                source_path: "chart/templates/z.yaml",
            },
        ];
        let documents = execute_documents(&sources, &db);
        let second = documents.first().ok_or_eyre("second file missing")?;
        sim_assert_eq!(have: second.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
        assert!(
            second
                .exits
                .iter()
                .filter(|exit| matches!(exit.flow, Flow::Normal(_)))
                .all(|exit| exit.state.read_local("fileLocal").is_none())
        );
        let expected = Predicate::Or(vec![
            Predicate::from(Guard::TypeIs {
                path: ValuesPath::parse("obj"),
                schema_type: "object".to_owned(),
            })
            .negated(),
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse("obj"),
                value: helm_schema_core::GuardValue::Null,
            }),
            Predicate::from(Guard::Absent {
                path: ValuesPath::parse("obj"),
            }),
        ]);
        let observed = Predicate::Or(
            documents
                .iter()
                .map(super::super::DocumentExecution::abort_predicate)
                .collect(),
        )
        .normalize_boolean();
        crate::value::tests::runtime_guard_json_domain::equal_on_models(
            &observed,
            &expected,
            &crate::value::tests::runtime_guard_json_domain::single_input_models(),
        );
        Ok(())
    }

    #[test]
    fn helper_calls_keep_distinct_execution_occurrences_from_each_other_and_the_file()
    -> eyre::Result<()> {
        let mut defines = DefineIndex::new();
        defines.add_file_source(
            "chart/templates/_helpers.tpl",
            r#"{{ define "marker" }}{{ "marked" }}{{ end }}"#,
        );
        let db = IrAnalysisDb::new(&defines);
        let sources = [SymbolicTemplate {
            source: r#"# {{ include "marker" . }}/{{ include "marker" . }}"#,
            source_path: "chart/templates/a.yaml",
        }];
        let documents = execute_documents(&sources, &db);
        let document = documents.first().ok_or_eyre("document missing")?;
        let helper_programs = document
            .all_sites()
            .filter(|site| {
                site.source_path() == Some("chart/templates/_helpers.tpl") && site.rendered()
            })
            .map(|site| site.program)
            .collect::<BTreeSet<_>>();
        sim_assert_eq!(have: helper_programs.len(), want: 2);
        let file_programs = document
            .exits
            .iter()
            .map(|exit| exit.state.program)
            .collect::<BTreeSet<_>>();
        assert!(helper_programs.is_disjoint(&file_programs));
        Ok(())
    }
}

mod partial_survivor_review_laws {
    use std::rc::Rc;

    use color_eyre::eyre::{self, OptionExt as _};
    use helm_schema_ast::DefineIndex;
    use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
    use test_util::prelude::sim_assert_eq;

    use crate::analysis_db::IrAnalysisDb;
    use crate::value::eval::Executor;
    use crate::value::{Coverage, Field, Flow, Remainder, State, Value};

    fn exact(state: &State, predicate: Predicate) -> Rc<crate::value::FrozenTest> {
        let rejected = predicate.negated();
        state
            .heap
            .arena
            .decision(predicate, rejected, Coverage::Exact)
    }

    #[test]
    fn partial_survivor_projection_retains_the_common_prefix_and_exact_necessary_tail_union()
    -> eyre::Result<()> {
        let state = State::new(Value::nil());
        let prefix = Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("x"),
            schema_type: "string".to_owned(),
        });
        let prefix_test = exact(&state, prefix.clone());
        let state = state.fork(prefix_test, true);
        let ancestor_predicate = Predicate::truthy_path("choose");
        let ancestor = exact(&state, ancestor_predicate.clone());
        let first = Predicate::truthy_path("first");
        let second = Predicate::truthy_path("second");
        let first_test = exact(&state, first.clone());
        let second_test = exact(&state, second.clone());
        let partial =
            state
                .heap
                .arena
                .decision(Predicate::False, Predicate::False, Coverage::Partial);
        let yes = state
            .fork(Rc::clone(&ancestor), true)
            .fork(partial, false)
            .fork(first_test, true);
        let no = state
            .fork(Rc::clone(&ancestor), false)
            .fork(second_test, true);
        let joined = State::join_branches(
            &yes.heap.arena.join_paths(&yes.active, &no.active),
            &yes,
            &no,
        )
        .ok_or_eyre("partial survivor join declined")?;
        let expected = Predicate::all(vec![
            prefix,
            Predicate::Or(vec![
                Predicate::all(vec![ancestor_predicate.clone(), first]),
                Predicate::all(vec![ancestor_predicate.negated(), second]),
            ]),
        ])
        .normalize_boolean();
        let projected = joined.path_predicate().normalize_boolean();
        assert!(
            projected.exactly_implies(&expected),
            "necessary prefix/tail condition dropped: {projected:?}"
        );
        assert!(
            expected.exactly_implies(&projected),
            "partial fact narrowed the exact projection: {projected:?}"
        );
        assert!(
            joined
                .active
                .iter()
                .any(|edge| edge.test.coverage == Coverage::Partial)
        );
        Ok(())
    }

    #[test]
    fn a_true_projected_union_keeps_a_fresh_partial_survival_marker() -> eyre::Result<()> {
        let state = State::new(Value::nil());
        let ancestor =
            state
                .heap
                .arena
                .decision(Predicate::False, Predicate::False, Coverage::Partial);
        let required = exact(&state, Predicate::truthy_path("required"));
        let yes = state
            .fork(Rc::clone(&ancestor), true)
            .fork(Rc::clone(&required), true);
        let no = state.fork(Rc::clone(&ancestor), false);
        let joined = State::join_branches(
            &yes.heap.arena.join_paths(&yes.active, &no.active),
            &yes,
            &no,
        )
        .ok_or_eyre("partial survivor join declined")?;
        sim_assert_eq!(have: joined.path_predicate().normalize_boolean(), want: Predicate::True);
        let markers = joined
            .active
            .iter()
            .filter(|edge| edge.test.coverage == Coverage::Partial)
            .collect::<Vec<_>>();
        assert!(
            !markers.is_empty(),
            "upper-bound True became complete survival"
        );
        assert!(
            markers
                .iter()
                .all(|edge| edge.test.id != ancestor.id && edge.test.id != required.id),
            "the marker reused an executed selector's identity"
        );
        Ok(())
    }

    #[test]
    fn a_fresh_partial_marker_does_not_specialize_a_live_selected_alias_or_its_later_writer()
    -> eyre::Result<()> {
        let mut state = State::new(Value::nil());
        let left = state.heap.map(Remainder::Closed);
        let right = state.heap.map(Remainder::Closed);
        let left_id = state.map_target(&left).ok_or_eyre("left map missing")?;
        let right_id = state.map_target(&right).ok_or_eyre("right map missing")?;
        state
            .heap
            .write(&left_id, "v", Field::Present(Value::string("left")));
        state
            .heap
            .write(&right_id, "v", Field::Present(Value::string("right")));
        state.bind("selected".to_owned(), left.clone());
        let ancestor = exact(&state, Predicate::truthy_path("choose"));
        let partial =
            state
                .heap
                .arena
                .decision(Predicate::False, Predicate::False, Coverage::Partial);
        let mut yes = state
            .fork(Rc::clone(&ancestor), true)
            .fork(Rc::clone(&partial), true);
        let mut no = state.fork(Rc::clone(&ancestor), false);
        yes.assign("selected", left.clone());
        no.assign("selected", right.clone());
        let joined = State::join_branches(
            &yes.heap.arena.join_paths(&yes.active, &no.active),
            &yes,
            &no,
        )
        .ok_or_eyre("partial alias join declined")?;
        let marker_ids = joined
            .active
            .iter()
            .filter(|edge| edge.test.coverage == Coverage::Partial)
            .map(|edge| edge.test.id)
            .collect::<Vec<_>>();
        assert!(
            marker_ids
                .iter()
                .all(|id| *id != ancestor.id && *id != partial.id)
        );
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let exits =
            Executor { db: &db }.body(r#"{{ $_ := set $selected "v" "changed" }}"#, None, joined);
        sim_assert_eq!(have: exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
        let normal = exits
            .iter()
            .find(|exit| matches!(exit.flow, Flow::Normal(_)))
            .ok_or_eyre("normal alias writer exit missing")?;
        for (taken, expected_left, expected_right) in
            [(true, "changed", "right"), (false, "left", "changed")]
        {
            let mut state = normal.state.fork(Rc::clone(&ancestor), taken);
            sim_assert_eq!(have: state.field(&left, "v").constant(), want: Some(&GuardValue::String(expected_left.to_owned())));
            sim_assert_eq!(have: state.field(&right, "v").constant(), want: Some(&GuardValue::String(expected_right.to_owned())));
        }
        Ok(())
    }
}

mod conditional_input_backing_laws {
    use std::rc::Rc;

    use color_eyre::eyre::{self, OptionExt as _};
    use helm_schema_ast::DefineIndex;
    use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
    use test_util::prelude::sim_assert_eq;

    use crate::analysis_db::IrAnalysisDb;
    use crate::symbolic::SymbolicTemplate;
    use crate::value::tests::execute_document;
    use crate::value::{
        Coverage, Datum, Field, Flow, Leaf, MapType, ObjectData, ObjectVersion, State,
        TypeCoverage, UnknownValue, Value, ValueNode, execute_documents,
    };

    fn typed_map(input: &Value) -> Value {
        let ValueNode::Leaf(Leaf { datum: _, origin }) = input.node.as_ref() else {
            return Value::unknown();
        };
        Value {
            node: Rc::new(ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(UnknownValue {
                    types: TypeCoverage::Map(MapType::Interface),
                    aliases: crate::value::Reach::Top,
                }),
                origin: origin.clone(),
            })),
        }
    }

    fn object(path: &str) -> Predicate {
        Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse(path),
            schema_type: "object".to_owned(),
        })
    }

    fn invalid_set_input(path: &str) -> Predicate {
        Predicate::Or(vec![
            object(path).negated(),
            Predicate::from(Guard::Eq {
                path: ValuesPath::parse(path),
                value: GuardValue::Null,
            }),
            Predicate::from(Guard::Absent {
                path: ValuesPath::parse(path),
            }),
        ])
    }

    #[test]
    fn one_sided_backing_keeps_a_stable_type_split_and_live_aliases() -> eyre::Result<()> {
        let state = State::new(Value::nil());
        let input = Value::input(ValuesPath::parse("obj"));
        let held = input.clone();
        let choice = state.heap.arena.decision(
            Predicate::truthy_path("flag"),
            Predicate::truthy_path("flag").negated(),
            Coverage::Exact,
        );
        let mut yes = state.fork(Rc::clone(&choice), true);
        let no = state.fork(Rc::clone(&choice), false);
        let target = yes
            .map_target(&typed_map(&input))
            .ok_or_eyre("map backing missing")?;
        yes.heap
            .write(&target, "v", Field::Present(Value::string("new")));
        let joined = State::join_branches(
            &yes.heap.arena.join_paths(&yes.active, &no.active),
            &yes,
            &no,
        )
        .ok_or_eyre("one-sided backing join declined")?;
        let backing = joined
            .heap
            .arena
            .input_objects
            .borrow()
            .get(&ValuesPath::parse("obj"))
            .cloned()
            .ok_or_eyre("canonical backing missing")?;
        sim_assert_eq!(have: &backing.object, want: &target);
        let first = joined.consumed(&input);
        let repeated = joined.consumed(&first);
        sim_assert_eq!(have: first, want: repeated);
        let positive = first.under(&backing.map_type, true);
        sim_assert_eq!(have: joined.heap.freeze(&positive).identity(), want: None);
        let negative = first.under(&backing.map_type, false);
        sim_assert_eq!(have: negative.identity(), want: input.identity());
        sim_assert_eq!(have: joined.consumed(&negative), want: negative.clone());
        assert!(matches!(
            negative.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(UnknownValue {
                    types: TypeCoverage::InputNonMap,
                    aliases: _
                }),
                origin: _,
            })
        ));
        let mut selected = joined
            .fork(Rc::clone(&backing.map_type), true)
            .fork(Rc::clone(&choice), true);
        let alias_target = selected
            .map_target(&held)
            .ok_or_eyre("captured alias lost")?;
        sim_assert_eq!(have: alias_target, want: target.clone());
        selected
            .heap
            .write(&alias_target, "v", Field::Present(Value::string("alias")));
        sim_assert_eq!(have: selected.field(&input, "v").constant(), want: Some(&GuardValue::String("alias".to_owned())));
        Ok(())
    }

    #[test]
    fn conditional_deletion_and_unresolved_writers_do_not_resurrect_original_presence()
    -> eyre::Result<()> {
        let mut state = State::new(Value::nil());
        let input = Value::input(ValuesPath::parse("obj"));
        let target = state
            .map_target(&typed_map(&input))
            .ok_or_eyre("map backing missing")?;
        let backing = state
            .heap
            .arena
            .input_objects
            .borrow()
            .get(&ValuesPath::parse("obj"))
            .cloned()
            .ok_or_eyre("canonical backing missing")?;
        let choice = state.heap.arena.decision(
            Predicate::truthy_path("flag"),
            Predicate::truthy_path("flag").negated(),
            Coverage::Exact,
        );
        let mut yes = state.fork(Rc::clone(&choice), true);
        let no = state.fork(Rc::clone(&choice), false);
        yes.heap.write(&target, "v", Field::Absent);
        let mut joined = State::join_branches(
            &yes.heap.arena.join_paths(&yes.active, &no.active),
            &yes,
            &no,
        )
        .ok_or_eyre("conditional deletion join declined")?;
        let presence = joined.field_presence(&input, "v");
        let absent = presence.under(&backing.map_type, true).under(&choice, true);
        sim_assert_eq!(have: absent.constant(), want: Some(&GuardValue::Bool(false)));
        joined.heap.install(
            target,
            ObjectVersion {
                revision: joined.heap.arena.snapshot(),
                multiplicity: crate::value::Multiplicity::Singleton,
                data: ObjectData::Widened {
                    inputs: Rc::new(input.inputs()),
                },
            },
        );
        let unknown = joined.field_presence(&input, "v");
        sim_assert_eq!(have: joined.test(&unknown).coverage, want: Coverage::Partial);
        assert!(joined.untouched_child_identity(&input, "v").is_none());
        Ok(())
    }

    #[test]
    fn map_backing_does_not_retype_scalar_or_nil_and_keeps_frozen_map_content() -> eyre::Result<()>
    {
        let mut state = State::new(Value::nil());
        let input = Value::input(ValuesPath::parse("obj"));
        let map_input = typed_map(&input);
        let target = state
            .map_target(&map_input)
            .ok_or_eyre("map backing missing")?;
        state
            .heap
            .write(&target, "v", Field::Present(Value::string("old")));
        let captured_map = state.consumed(&map_input);
        let frozen = state.heap.freeze(&captured_map);
        state
            .heap
            .write(&target, "v", Field::Present(Value::string("new")));
        let old = frozen
            .read_field("v")
            .present()
            .ok_or_eyre("old field missing")?;
        sim_assert_eq!(have: old.literal(), want: Some(&GuardValue::String("old".to_owned())));
        let scalar = Value {
            node: Rc::new(ValueNode::Leaf(Leaf {
                datum: Datum::Scalar {
                    go_type: crate::value::ScalarType::String,
                    content: None,
                },
                origin: crate::value::Origin {
                    derivation: None,
                    identity: Some(ValuesPath::parse("obj")),
                    influences: std::collections::BTreeSet::default(),
                },
            })),
        };
        sim_assert_eq!(have: state.consumed(&scalar), want: scalar);
        sim_assert_eq!(have: state.consumed(&Value::nil()), want: Value::nil());
        Ok(())
    }

    #[test]
    fn independent_optional_map_writers_across_files_have_one_normal_continuation() {
        for count in [1, 2, 4, 8] {
            let owned: Vec<_> = (0..count).map(|index| (
                format!("{{{{ if .Values.g{index} }}}}{{{{ $_ := set .Values.m{index} \"unused\" \"written\" }}}}{{{{ end }}}}"),
                format!("chart/templates/{index:02}.yaml"),
            )).collect();
            let sources: Vec<_> = owned
                .iter()
                .map(|(source, source_path)| SymbolicTemplate {
                    source,
                    source_path,
                })
                .collect();
            let db = IrAnalysisDb::new(&DefineIndex::new());
            let documents = execute_documents(&sources, &db);
            for document in &documents {
                sim_assert_eq!(have: document.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
            }
            let observed = Predicate::Or(
                documents
                    .iter()
                    .map(super::super::DocumentExecution::abort_predicate)
                    .collect(),
            )
            .normalize_boolean();
            let expected = Predicate::Or(
                (0..count)
                    .map(|index| {
                        Predicate::all(vec![
                            Predicate::truthy_path(format!("g{index}")),
                            invalid_set_input(&format!("m{index}")),
                        ])
                    })
                    .collect(),
            )
            .normalize_boolean();
            crate::value::tests::runtime_guard_json_domain::equal_on_models(
                &observed,
                &expected,
                &crate::value::tests::runtime_guard_json_domain::independent_input_models(count),
            );
        }
    }

    #[test]
    fn scalar_consumer_after_optional_set_keeps_only_the_untouched_scalar_arm() {
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(
            r#"{{ if .Values.flag }}{{ $_ := set .Values.obj "unused" "written" }}{{ end }}{{ trim .Values.obj }}"#,
            None,
            &db,
        );
        let expected = Predicate::Or(vec![
            Predicate::truthy_path("flag"),
            Predicate::from(Guard::TypeIs {
                path: ValuesPath::parse("obj"),
                schema_type: "string".to_owned(),
            })
            .negated(),
        ])
        .normalize_boolean();
        let observed = execution.abort_predicate();
        crate::value::tests::runtime_guard_json_domain::equal_on_models(
            &observed,
            &expected,
            &crate::value::tests::runtime_guard_json_domain::single_input_models(),
        );
        sim_assert_eq!(have: execution.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
    }
}

mod runtime_guard_json_domain {
    use helm_schema_core::{Guard, GuardValue, Predicate, PredicateKind};
    use serde_json::Value;
    use test_util::prelude::sim_assert_eq;

    fn evaluate(predicate: &Predicate, input: &Value) -> Option<bool> {
        match predicate.kind() {
            PredicateKind::True => Some(true),
            PredicateKind::False => Some(false),
            PredicateKind::Not(inner) => Some(!evaluate(inner, input)?),
            PredicateKind::And(parts) => {
                let mut result = true;
                for part in parts {
                    result &= evaluate(part, input)?;
                }
                Some(result)
            }
            PredicateKind::Or(parts) => {
                let mut result = false;
                for part in parts {
                    result |= evaluate(part, input)?;
                }
                Some(result)
            }
            PredicateKind::Approximate { .. } => None,
            PredicateKind::Guard(guard) => match guard {
                Guard::Truthy { path } => {
                    Some(input.get(path.encode()).is_some_and(|value| match value {
                        Value::Null => false,
                        Value::Bool(value) => *value,
                        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
                        Value::String(value) => !value.is_empty(),
                        Value::Array(value) => !value.is_empty(),
                        Value::Object(value) => !value.is_empty(),
                    }))
                }
                Guard::Absent { path } => Some(input.get(path.encode()).is_none()),
                Guard::Eq {
                    path,
                    value: GuardValue::Null,
                } => Some(input.get(path.encode()).is_none_or(Value::is_null)),
                Guard::TypeIs { path, schema_type } => match schema_type.as_str() {
                    "object" => Some(input.get(path.encode()).is_some_and(Value::is_object)),
                    "string" => Some(input.get(path.encode()).is_some_and(Value::is_string)),
                    _ => None,
                },
                _ => None,
            },
        }
    }

    pub(super) fn equal_on_models(have: &Predicate, want: &Predicate, models: &[Value]) {
        for model in models {
            let have = evaluate(have, model);
            let want = evaluate(want, model);
            assert!(
                have.is_some() && want.is_some(),
                "unsupported fixture guard: {model}"
            );
            sim_assert_eq!(have: have, want: want);
        }
    }

    pub(super) fn single_input_models() -> Vec<Value> {
        let mut models = Vec::new();
        for flag in [false, true] {
            for value in [
                None,
                Some(serde_json::json!(null)),
                Some(serde_json::json!("")),
                Some(serde_json::json!("scalar")),
                Some(serde_json::json!(0)),
                Some(serde_json::json!(7)),
                Some(serde_json::json!(false)),
                Some(serde_json::json!(true)),
                Some(serde_json::json!([])),
                Some(serde_json::json!([1])),
                Some(serde_json::json!({})),
                Some(serde_json::json!({"x":"old"})),
            ] {
                let mut model = serde_json::json!({"flag":flag});
                if let Some(value) = value {
                    model["obj"] = value;
                }
                models.push(model);
            }
        }
        models
    }

    pub(super) fn independent_input_models(count: usize) -> Vec<Value> {
        let mut models = Vec::new();
        for flags in 0..(1_usize << count) {
            let mut base = serde_json::Map::new();
            for index in 0..count {
                base.insert(format!("g{index}"), Value::Bool(flags & (1 << index) != 0));
                base.insert(format!("m{index}"), serde_json::json!({}));
            }
            models.push(Value::Object(base.clone()));
            for index in 0..count {
                for value in [
                    None,
                    Some(serde_json::json!(null)),
                    Some(serde_json::json!("scalar")),
                    Some(serde_json::json!(7)),
                    Some(serde_json::json!([])),
                ] {
                    let mut model = base.clone();
                    let key = format!("m{index}");
                    if let Some(value) = value {
                        model.insert(key, value);
                    } else {
                        model.remove(&key);
                    }
                    models.push(Value::Object(model));
                }
            }
        }
        models
    }
}

mod runtime_reachability_laws {
    use std::rc::Rc;

    use helm_schema_core::{GuardValue, Predicate};
    use test_util::prelude::sim_assert_eq;

    use crate::value::{Coverage, DecisionEdge, State, Value};

    #[test]
    fn partial_survivors_keep_every_original_decision_assignment() {
        let initial = State::new(Value::nil());
        let arena = &initial.heap.arena;
        let atoms: [_; 3] = std::array::from_fn(|_| {
            arena.decision(Predicate::False, Predicate::False, Coverage::Partial)
        });
        let [branch, yes_survives, no_survives] = &atoms;
        let yes = vec![
            DecisionEdge {
                test: Rc::clone(branch),
                taken: true,
            },
            DecisionEdge {
                test: Rc::clone(yes_survives),
                taken: true,
            },
        ];
        let no = vec![
            DecisionEdge {
                test: Rc::clone(branch),
                taken: false,
            },
            DecisionEdge {
                test: Rc::clone(no_survives),
                taken: true,
            },
        ];
        let joined = arena.surviving_decision(&yes, &no);
        // Opaque tests have identical schema projections but independent runtime identities.
        for assignment in 0_u8..8 {
            let edges: Vec<_> = atoms
                .iter()
                .enumerate()
                .map(|(index, test)| DecisionEdge {
                    test: Rc::clone(test),
                    taken: assignment & (1 << index) != 0,
                })
                .collect();
            let selected = arena.runtime_path(&edges);
            let expected = if assignment & 1 != 0 {
                assignment & 2 != 0
            } else {
                assignment & 4 != 0
            };
            sim_assert_eq!(have: selected.implies_test(&joined), want: Some(expected));
            let mut state = initial.clone();
            state.active = edges.into();
            std::rc::Rc::make_mut(&mut state.active).push(DecisionEdge {
                test: Rc::clone(&joined),
                taken: true,
            });
            sim_assert_eq!(have: state.path_predicate() == Predicate::False, want: !expected);
        }
    }

    #[test]
    fn surviving_guard_and_later_choice_specialize_the_same_value() {
        let initial = State::new(Value::nil());
        let arena = &initial.heap.arena;
        let first = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
        let second = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
        let survives = arena.surviving_decision(
            &[DecisionEdge {
                test: Rc::clone(&first),
                taken: true,
            }],
            &[DecisionEdge {
                test: Rc::clone(&second),
                taken: true,
            }],
        );
        let value = Value::select(Rc::clone(&first), Value::string("yes"), Value::string("no"));
        let state = initial.fork(survives, true).fork(second, false);
        sim_assert_eq!(have: state.consumed(&value).constant().cloned(), want: Some(GuardValue::String("yes".to_owned())));
    }

    #[test]
    fn opposite_opaque_arms_reconverge_without_a_new_unknown_choice() {
        let initial = State::new(Value::nil());
        let arena = &initial.heap.arena;
        let choice = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
        let survives = arena.surviving_decision(
            &[DecisionEdge {
                test: Rc::clone(&choice),
                taken: true,
            }],
            &[DecisionEdge {
                test: choice,
                taken: false,
            }],
        );
        sim_assert_eq!(have: survives.constant_outcome(), want: Some(true));
        sim_assert_eq!(have: survives.coverage, want: Coverage::Exact);
    }
}
mod window_buffer_review_laws {
    use std::rc::Rc;

    use color_eyre::eyre::{self, OptionExt as _};
    use helm_schema_core::Predicate;
    use helm_schema_syntax::Span;
    use test_util::prelude::sim_assert_eq;

    use crate::value::window_exits::{WindowBuffer, WindowChunk, WindowExits};
    use crate::value::{
        AbortReason, Arena, Coverage, Exit, ExitBuffer, Flow, Normal, State, Value,
    };

    #[derive(Debug, PartialEq, Eq)]
    struct Allocations {
        objects: u64,
        snapshots: u64,
        decisions: u64,
    }

    fn allocations(arena: &Arena) -> Allocations {
        Allocations {
            objects: arena.next_object.get(),
            snapshots: arena.next_snapshot.get(),
            decisions: arena.next_decision.get(),
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ExitSignature {
        program: u64,
        flow: String,
        value: Option<Value>,
        heap_snapshot: u64,
        output: Value,
        active: Vec<(u64, bool, Coverage)>,
        text_observations: Vec<(u64, String, Predicate)>,
        invocation_records_complete: bool,
    }

    fn signatures(exits: &[Exit]) -> Vec<ExitSignature> {
        exits
            .iter()
            .map(|exit| ExitSignature {
                program: exit.state.program,
                flow: match &exit.flow {
                    Flow::Normal(_) => "normal".to_owned(),
                    Flow::Abort(reason) => format!("abort:{reason:?}"),
                    Flow::Break(id) => format!("break:{id:?}"),
                    Flow::Continue(id) => format!("continue:{id:?}"),
                },
                value: match &exit.flow {
                    Flow::Normal(normal) => Some(normal.value.clone()),
                    Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => None,
                },
                heap_snapshot: exit.state.heap.version.id.ordinal,
                output: exit.state.output.as_value(),
                active: exit
                    .state
                    .active
                    .iter()
                    .map(|edge| (edge.test.id, edge.taken, edge.test.coverage))
                    .collect(),
                text_observations: exit
                    .state
                    .text_sites
                    .iter()
                    .map(|site| (site.program, site.bytes.clone(), site.condition()))
                    .collect(),
                invocation_records_complete: exit.state.invocation_records_complete,
            })
            .collect()
    }

    fn terminal_fixture(count: usize) -> (Rc<Arena>, Vec<Exit>) {
        let state = State::new(Value::nil());
        let arena = Rc::clone(&state.heap.arena);
        let exits = (0..count)
            .map(|index| {
                let mut state = state.clone();
                state.program = index as u64;
                state.emit_bytes(Span::new(0, 3), Some("templates/test.yaml"), "old");
                let id = crate::value::LoopId {
                    call: 0,
                    ordinal: 7,
                };
                let flow = match index % 3 {
                    0 => Flow::Abort(AbortReason::Fail),
                    1 => Flow::Break(id),
                    _ => Flow::Continue(id),
                };
                Exit { state, flow }
            })
            .collect();
        (arena, exits)
    }

    fn forward_all(buffer: &mut WindowBuffer, exits: Vec<Exit>) {
        for chunk in WindowExits::new(exits).into_chunks() {
            match chunk {
                WindowChunk::Terminal(run) => buffer.forward(run),
                WindowChunk::Single(exit) => buffer.push(*exit),
            }
        }
    }

    fn compact_buffer() -> WindowBuffer {
        WindowExits::new(Vec::new()).next_buffer()
    }

    #[test]
    fn old_terminal_run_boundary_matches_every_sequential_budget_charge() -> eyre::Result<()> {
        for count in [4095, 4096, 4097] {
            let (legacy_arena, legacy_inputs) = terminal_fixture(count);
            let (compact_arena, compact_inputs) = terminal_fixture(count);
            let legacy_seed = legacy_inputs
                .first()
                .ok_or_eyre("legacy seed missing")?
                .state
                .clone();
            let compact_seed = compact_inputs
                .first()
                .ok_or_eyre("compact seed missing")?
                .state
                .clone();
            let mut legacy = ExitBuffer::default();
            let mut compact = compact_buffer();
            legacy.extend(legacy_inputs);
            forward_all(&mut compact, compact_inputs);
            sim_assert_eq!(have: allocations(&compact_arena), want: allocations(&legacy_arena));
            for program in [50_000, 50_001] {
                let mut legacy_state = legacy_seed.clone();
                legacy_state.program = program;
                let mut compact_state = compact_seed.clone();
                compact_state.program = program;
                legacy.push(Exit {
                    state: legacy_state,
                    flow: Flow::Abort(AbortReason::Type),
                });
                compact.push(Exit {
                    state: compact_state,
                    flow: Flow::Abort(AbortReason::Type),
                });
                sim_assert_eq!(have: allocations(&compact_arena), want: allocations(&legacy_arena));
            }
            let expected = legacy.finish();
            let actual = compact.finish().into_exits();
            sim_assert_eq!(have: signatures(&actual), want: signatures(&expected));
        }
        Ok(())
    }

    #[test]
    fn new_output_crossing_allocates_before_a_later_normal_action() -> eyre::Result<()> {
        let (legacy_arena, legacy_inputs) = terminal_fixture(4095);
        let (compact_arena, compact_inputs) = terminal_fixture(4095);
        let legacy_seed = legacy_inputs
            .first()
            .ok_or_eyre("legacy seed missing")?
            .state
            .clone();
        let compact_seed = compact_inputs
            .first()
            .ok_or_eyre("compact seed missing")?
            .state
            .clone();
        let mut legacy = ExitBuffer::default();
        let mut compact = compact_buffer();
        legacy.extend(legacy_inputs);
        forward_all(&mut compact, compact_inputs);
        let before = allocations(&compact_arena);
        for program in [50_000, 50_001] {
            let mut expected = legacy_seed.clone();
            expected.program = program;
            let mut actual = compact_seed.clone();
            actual.program = program;
            legacy.push(Exit {
                state: expected,
                flow: Flow::Normal(Normal {
                    value: Value::string("new"),
                }),
            });
            compact.push(Exit {
                state: actual,
                flow: Flow::Normal(Normal {
                    value: Value::string("new"),
                }),
            });
            sim_assert_eq!(have: allocations(&compact_arena), want: allocations(&legacy_arena));
            if program == 50_000 {
                sim_assert_eq!(have: allocations(&compact_arena), want: before);
            }
        }
        assert!(compact_arena.next_decision.get() > before.decisions);
        let mut later_expected = legacy_seed;
        let mut later_actual = compact_seed;
        let expected_map = later_expected.heap.map(crate::value::Remainder::Closed);
        let actual_map = later_actual.heap.map(crate::value::Remainder::Closed);
        sim_assert_eq!(have: actual_map, want: expected_map);
        sim_assert_eq!(have: allocations(&compact_arena), want: allocations(&legacy_arena));
        sim_assert_eq!(have: signatures(&compact.finish().into_exits()), want: signatures(&legacy.finish()));
        Ok(())
    }

    #[test]
    fn crossing_inside_an_old_run_happens_before_the_next_live_action() -> eyre::Result<()> {
        let (legacy_arena, legacy_inputs) = terminal_fixture(4097);
        let (compact_arena, compact_inputs) = terminal_fixture(4097);
        let mut legacy_state = legacy_inputs
            .first()
            .ok_or_eyre("legacy seed missing")?
            .state
            .clone();
        let mut compact_state = compact_inputs
            .first()
            .ok_or_eyre("compact seed missing")?
            .state
            .clone();
        let mut legacy = ExitBuffer::default();
        let mut compact = compact_buffer();
        legacy.extend(legacy_inputs);
        forward_all(&mut compact, compact_inputs);
        assert!(compact_arena.next_decision.get() > 0);
        sim_assert_eq!(have: allocations(&compact_arena), want: allocations(&legacy_arena));
        sim_assert_eq!(have: compact_state.heap.map(crate::value::Remainder::Closed), want: legacy_state.heap.map(crate::value::Remainder::Closed));
        sim_assert_eq!(have: allocations(&compact_arena), want: allocations(&legacy_arena));
        Ok(())
    }

    fn interleaved_join_fixture() -> Vec<Exit> {
        let state = State::new(Value::nil());
        let decision = state.heap.arena.decision(
            Predicate::truthy_path("flag"),
            Predicate::truthy_path("flag").negated(),
            Coverage::Exact,
        );
        vec![
            Exit {
                state: state.fork(Rc::clone(&decision), true),
                flow: Flow::Normal(Normal {
                    value: Value::string("yes"),
                }),
            },
            Exit {
                state: state.clone(),
                flow: Flow::Abort(AbortReason::Fail),
            },
            Exit {
                state: state.fork(decision, false),
                flow: Flow::Normal(Normal {
                    value: Value::string("no"),
                }),
            },
        ]
    }

    #[test]
    fn normal_only_reconvergence_keeps_the_later_slot_after_an_intervening_terminal() {
        let mut legacy = ExitBuffer::default();
        legacy.extend(interleaved_join_fixture());
        let mut compact = compact_buffer();
        forward_all(&mut compact, interleaved_join_fixture());
        let expected = legacy.finish();
        let actual = compact.finish().into_exits();
        sim_assert_eq!(have: actual.len(), want: 2);
        assert!(matches!(
            actual.first().map(|exit| &exit.flow),
            Some(Flow::Abort(AbortReason::Fail))
        ));
        assert!(matches!(
            actual.get(1).map(|exit| &exit.flow),
            Some(Flow::Normal(_))
        ));
        sim_assert_eq!(have: signatures(&actual), want: signatures(&expected));
    }
}

mod value_transfer_laws {
    use std::rc::Rc;

    use color_eyre::eyre::{self, OptionExt as _};
    use helm_schema_ast::DefineIndex;
    use helm_schema_core::{GuardValue, Predicate};
    use test_util::prelude::sim_assert_eq;

    use crate::analysis_db::IrAnalysisDb;
    use crate::value::eval::Executor;
    use crate::value::{
        ArgumentForm, Callee, CompletionInput, Coverage, Flow, Invocation, Operand, State, Value,
    };

    #[test]
    fn correlated_scalar_arguments_transfer_without_forking_the_heap() -> eyre::Result<()> {
        let state = State::new(Value::nil());
        state.heap.freeze(&Value::string("warm"));
        let arena = Rc::clone(&state.heap.arena);
        let heap = Rc::clone(&state.heap.version);
        let decision = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
        let before = arena.next_snapshot.get();
        let values = [
            Value::select(Rc::clone(&decision), Value::string("A"), Value::string("a")),
            Value::select(Rc::clone(&decision), Value::string("B"), Value::string("b")),
        ];
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let exits = Executor { db: &db }
            .complete(CompletionInput::Invocation {
                invocation: Invocation {
                    callee: Callee::Function("print".to_owned()),
                    operands: values
                        .into_iter()
                        .map(|value| Operand {
                            value,
                            site: 7,
                            form: ArgumentForm::Evaluated,
                        })
                        .collect(),
                    site: 7,
                },
                state,
            })
            .into_exits();
        sim_assert_eq!(have: exits.len(), want: 1);
        let exit = exits.first().ok_or_eyre("missing scalar transfer")?;
        let Flow::Normal(normal) = &exit.flow else {
            eyre::bail!("scalar transfer aborted");
        };
        // Each argument selects the same original opaque choice; crossed outputs are impossible.
        sim_assert_eq!(have: normal.value.under(&decision, true).constant().cloned(), want: Some(GuardValue::String("AB".to_owned())));
        sim_assert_eq!(have: normal.value.under(&decision, false).constant().cloned(), want: Some(GuardValue::String("ab".to_owned())));
        assert!(Rc::ptr_eq(&heap, &exit.state.heap.version));
        sim_assert_eq!(have: arena.next_snapshot.get(), want: before);
        Ok(())
    }

    #[test]
    fn parameter_alternatives_create_only_normal_and_abort_states() -> eyre::Result<()> {
        let state = State::new(Value::nil());
        state.heap.freeze(&Value::string("warm"));
        let arena = Rc::clone(&state.heap.arena);
        let mut values: Vec<_> = (0..8)
            .map(|index| {
                if index % 2 == 0 {
                    Value::string("valid")
                } else {
                    Value::literal(GuardValue::Int(1))
                }
            })
            .collect();
        while values.len() > 1 {
            values = values
                .chunks(2)
                .filter_map(|pair| {
                    let [yes, no] = pair else {
                        return None;
                    };
                    let decision =
                        arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
                    Some(Value::select(decision, yes.clone(), no.clone()))
                })
                .collect();
        }
        let value = values
            .pop()
            .ok_or_eyre("missing constructed parameter alternatives")?;
        let before = arena.next_snapshot.get();
        let exits = crate::value::kinds::validate_parameter(
            state,
            value,
            Some(crate::value::kinds::GoType::String),
            ArgumentForm::Evaluated,
        );
        sim_assert_eq!(have: exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
        sim_assert_eq!(have: exits.iter().filter(|exit| matches!(exit.flow, Flow::Abort(_))).count(), want: 1);
        sim_assert_eq!(have: arena.next_snapshot.get(), want: before);
        Ok(())
    }

    #[test]
    fn whole_container_reads_cover_the_unvisited_dispatch_tail() -> eyre::Result<()> {
        use std::collections::BTreeSet;

        use helm_schema_core::ValuesPath;

        use crate::value::ReadCoverage;

        let state = State::new(Value::nil());
        let paths: BTreeSet<_> = (0..256)
            .map(|index| ValuesPath::parse(&format!("input{index}")))
            .collect();
        let mut values: Vec<_> = paths.iter().cloned().map(Value::input).collect();
        while values.len() > 1 {
            let mut joined = Vec::new();
            for pair in values.chunks(2) {
                let [yes, no] = pair else {
                    eyre::bail!("expected balanced value tree");
                };
                let decision = state.heap.arena.decision(
                    Predicate::False,
                    Predicate::False,
                    Coverage::Partial,
                );
                joined.push(Value::select(decision, yes.clone(), no.clone()));
            }
            values = joined;
        }
        let value = values.pop().ok_or_eyre("missing dispatch alternatives")?;
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let exits = Executor { db: &db }
            .complete(CompletionInput::Invocation {
                invocation: Invocation {
                    callee: Callee::Function("toYaml".to_owned()),
                    operands: vec![Operand {
                        value,
                        site: 9,
                        form: ArgumentForm::Evaluated,
                    }],
                    site: 9,
                },
                state,
            })
            .into_exits();
        sim_assert_eq!(have: exits.len(), want: 1);
        let exit = exits
            .first()
            .ok_or_eyre("missing whole-container transfer")?;
        assert!(matches!(exit.flow, Flow::Normal(_)));
        let covered: BTreeSet<_> = exit
            .state
            .reads
            .iter()
            .filter_map(|read| match read.coverage {
                ReadCoverage::UnknownRegion => Some(read.path.clone()),
                ReadCoverage::ExactPath
                | ReadCoverage::DefaultFallback
                | ReadCoverage::RootMergeFallback
                | ReadCoverage::OpaqueValuesRoot
                | ReadCoverage::SerializedInfluence
                | ReadCoverage::MemberHost { .. }
                | ReadCoverage::MapArgument { .. } => None,
            })
            .collect();
        sim_assert_eq!(have: covered, want: paths);
        Ok(())
    }
}

#[path = "runtime_comparisons.rs"]
mod comparisons;

#[path = "runtime_join_maps.rs"]
mod join_map_laws;

#[test]
fn selected_root_body_keeps_defining_source_offsets_and_resource_identity() -> eyre::Result<()> {
    use crate::symbolic::{SymbolicIrContext, SymbolicTemplate};
    use helm_schema_core::{ContractProvenance, ResourceRef, SourceSpan, ValuesPath};

    let defining_path = "chart/templates/a.tpl";
    let scheduled_path = "chart/templates/z.yaml";
    let source = indoc::indoc! {r#"
        {{- define "chart/templates/z.yaml" -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: selected
        data:
          payload: {{ .Values.payload }}
        {{- end -}}
    "#};
    let expression = "{{ .Values.payload }}";
    let start = source
        .find(expression)
        .ok_or_eyre("payload expression missing")?;
    let mut defines = DefineIndex::new();
    defines.add_file_source(defining_path, source);
    let db = IrAnalysisDb::new(&defines);
    let defining = db.parsed_source(source, Some(defining_path), defining_path);
    let body = defining
        .declarations
        .iter()
        .find(|definition| definition.name == scheduled_path)
        .ok_or_eyre("named body program missing")?;
    assert!(body.program.body_offset() > 0);
    sim_assert_eq!(have: body.program.source_path(), want: Some(defining_path));
    sim_assert_eq!(
        have: body.program.source().find(expression).map(|offset| offset + body.program.body_offset()),
        want: Some(start)
    );

    let context = SymbolicIrContext::new(&defines);
    let mut batch = context.generate_contract_irs_for_sources(&[SymbolicTemplate {
        source: "not selected",
        source_path: scheduled_path,
    }]);
    sim_assert_eq!(have: batch.len(), want: 1);
    let batch = batch
        .pop()
        .ok_or_eyre("scheduled source contract missing")?;
    let single = context.generate_contract_ir_for_source("not selected", scheduled_path);
    for contract in [batch, single] {
        let finalized = contract.finalize();
        let uses: Vec<_> = finalized
            .uses()
            .iter()
            .filter(|use_| {
                use_.source_expr == ValuesPath::parse("payload") && use_.resource.is_some()
            })
            .collect();
        assert!(!uses.is_empty(), "{:#?}", finalized.uses());
        let resource = ResourceRef::concrete("v1".to_string(), "ConfigMap".to_string());
        for use_ in &uses {
            sim_assert_eq!(have: use_.resource.as_ref(), want: Some(&resource));
        }
        let provenance: BTreeSet<_> = uses
            .iter()
            .flat_map(|use_| use_.provenance.iter().cloned())
            .collect();
        sim_assert_eq!(have: provenance, want: BTreeSet::from([ContractProvenance::new(
            defining_path,
            SourceSpan::new(start, start + expression.len()),
            Vec::new(),
        )]));
    }
    Ok(())
}

#[test]
fn sealed_projection_releases_observation_snapshots_before_the_next_file() -> eyre::Result<()> {
    use std::rc::{Rc, Weak};

    let db = IrAnalysisDb::new(&DefineIndex::new());
    let sources = vec![
        db.parsed_source(
            "later",
            Some("chart/templates/a.txt"),
            "chart/templates/a.txt",
        ),
        db.parsed_source(
            r#"{{ $m := dict "key" "old" }}{{ $m }}{{ $_ := set $m "key" "new" }}"#,
            Some("chart/templates/z.txt"),
            "chart/templates/z.txt",
        ),
    ];
    let mut previous: Option<Weak<crate::value::ValueNode>> = None;
    let mut order = Vec::new();
    let mut checks = Vec::new();
    crate::value::project_documents(sources, &db, |index, _, execution| {
        let check = (|| -> eyre::Result<usize> {
            order.push(index);
            match index {
                1 => {
                    // The emission pins the old map while the live heap advances.
                    let site = execution
                        .sites()
                        .find(|site| site.value().json() == Some(serde_json::json!({"key": "old"})))
                        .ok_or_eyre("frozen old map observation missing")?;
                    let snapshot = Rc::downgrade(&site.value().value.node);
                    assert!(snapshot.upgrade().is_some());
                    previous = Some(snapshot);
                }
                0 => {
                    // The preceding callback's borrowed document has now been released.
                    assert!(
                        previous
                            .as_ref()
                            .is_some_and(|snapshot| snapshot.upgrade().is_none())
                    );
                }
                _ => eyre::bail!("unexpected source index {index}"),
            }
            Ok(index)
        })();
        checks.push(check);
    });
    sim_assert_eq!(have: order, want: vec![1, 0]);
    let mut projected = checks.into_iter().collect::<eyre::Result<Vec<_>>>()?;
    projected.sort_unstable();
    sim_assert_eq!(have: projected, want: vec![0, 1]);
    Ok(())
}

impl super::FrozenValue {
    pub(super) fn json(&self) -> Option<serde_json::Value> {
        self.json_with(&mut super::output::JsonQuery {
            preserve_nil_collections: false,
            remaining: None,
            outcome: &|_| None,
            choice: None,
        })
    }
}

struct HostChoice {
    guard: helm_schema_core::CapabilityGuard,
    test: Rc<super::FrozenTest>,
}

pub(crate) fn conditional_helper_text(
    execution: &DocumentExecution,
) -> helm_schema_core::HelperBranchBody {
    let mut hosts = std::collections::BTreeMap::new();
    for site in execution.all_sites() {
        if let Some(super::relationship::Derivation {
            source: _,
            operation: super::relationship::ValueOperation::Capability { guard, decision },
        }) = site.value().value.operation_preimage()
        {
            hosts.insert(
                decision.id,
                HostChoice {
                    guard: guard.clone(),
                    test: Rc::clone(decision),
                },
            );
        }
    }
    let Some(exit) = execution.exits.first() else {
        return helm_schema_core::HelperBranchBody::literals(Vec::new());
    };
    let hosts = hosts.into_values().collect::<Vec<_>>();
    let mut bodies = Vec::new();
    let mut remaining = 4096;
    for (_, buffer) in execution.text() {
        let body = conditional_hosts(buffer, &hosts, &exit.state.heap.arena, &[], &mut remaining);
        bodies.push(body);
    }
    if bodies.len() == 1 {
        return bodies.remove(0);
    }
    helm_schema_core::HelperBranchBody::literals(
        bodies
            .iter()
            .flat_map(helm_schema_core::HelperBranchBody::all_literals)
            .collect(),
    )
}

fn conditional_hosts(
    buffer: &super::TextBuffer,
    hosts: &[HostChoice],
    arena: &super::Arena,
    choices: &[super::DecisionEdge],
    remaining: &mut usize,
) -> helm_schema_core::HelperBranchBody {
    use helm_schema_core::{HelperBranch, HelperBranchBody};
    if *remaining == 0 || choices.len() >= 64 {
        return HelperBranchBody::literals(Vec::new());
    }
    *remaining -= 1;
    let Some((host, rest)) = hosts.split_first() else {
        return conditional_buffer(buffer, arena, choices, remaining);
    };
    let mut branch = |taken| {
        let mut selected = choices.to_vec();
        selected.push(super::DecisionEdge {
            test: Rc::clone(&host.test),
            taken,
        });
        conditional_hosts(buffer, rest, arena, &selected, remaining)
    };
    let yes = branch(true);
    let no = branch(false);
    if yes == no {
        return yes;
    }
    let mut branches = vec![HelperBranch {
        guard: Some(host.guard.clone()),
        body: yes,
    }];
    match no {
        HelperBranchBody::Nested { branches: rest } => branches.extend(rest),
        no @ HelperBranchBody::Literals { values: _ } => branches.push(HelperBranch {
            guard: None,
            body: no,
        }),
    }
    HelperBranchBody::Nested { branches }
}

fn conditional_buffer(
    buffer: &super::TextBuffer,
    arena: &super::Arena,
    choices: &[super::DecisionEdge],
    remaining: &mut usize,
) -> helm_schema_core::HelperBranchBody {
    use helm_schema_core::HelperBranchBody;
    let empty = || HelperBranchBody::literals(Vec::new());
    if choices.len() >= 64 || *remaining == 0 {
        return empty();
    }
    let selected = arena.runtime_path(choices);
    match super::text::buffer_text(buffer, &|test| selected.implies_test(test), remaining) {
        Ok(text) => {
            let value = serde_yaml::from_str::<serde_json::Value>(&text).ok();
            let text = match value {
                Some(serde_json::Value::String(text)) => text,
                Some(serde_json::Value::Number(value)) => value.to_string(),
                Some(serde_json::Value::Bool(value)) => value.to_string(),
                Some(
                    serde_json::Value::Null
                    | serde_json::Value::Array(_)
                    | serde_json::Value::Object(_),
                )
                | None => return empty(),
            };
            HelperBranchBody::literals(vec![text])
        }
        Err(super::text::Unresolved::Choice(test)) => {
            let mut branch = |taken| {
                let mut edges = choices.to_vec();
                edges.push(super::DecisionEdge {
                    test: Rc::clone(&test),
                    taken,
                });
                conditional_buffer(buffer, arena, &edges, remaining)
            };
            let yes = branch(true);
            let no = branch(false);
            let mut values = yes.all_literals();
            for text in no.all_literals() {
                if !values.contains(&text) {
                    values.push(text);
                }
            }
            HelperBranchBody::literals(values)
        }
        Err(super::text::Unresolved::Unknown | super::text::Unresolved::Exhausted) => empty(),
    }
}

#[test]
fn printf_renders_exact_typed_semver_components() {
    sim_assert_eq!(have: texts(r#"{{ printf "%d.%d.0" 1 35 }}"#), want: vec![Some("1.35.0".to_owned())]);
    sim_assert_eq!(have: texts(r#"{{ printf "%d" "35" }}"#), want: vec![None], "a string must not masquerade as a decimal argument");
}

#[test]
fn printf_exact_rendering_only_accepts_supported_string_formats() {
    for (source, expected) in [
        (r#"{{ printf "prefix-%s-%%" "x" }}"#, Some("prefix-x-%")),
        (r#"{{ printf "%d" "x" }}"#, None),
        (r#"{{ printf "literal" "unused" }}"#, None),
        (r#"{{ printf "%s-%s" "x" }}"#, None),
    ] {
        sim_assert_eq!(have: texts(source), want: vec![expected.map(str::to_owned)], "{source}");
    }
}
