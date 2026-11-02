use std::fmt::Write as _;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{GuardValue, Predicate};
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::symbolic::SymbolicTemplate;
use crate::value::guard::RuntimeGuard;
use crate::value::{
    Coverage, DecisionEdge, Exit, Field, Flow, Normal, Remainder, State, Value, ValueNode,
    execute_documents,
};

#[test]
fn runtime_guard_stays_exact_across_projection_limits() {
    let initial = State::new(Value::nil());
    let arena = &initial.heap.arena;
    // Exceed the output certificate even when normalization retains more exact formulas.
    let ambient = Predicate::all(
        (0..5000)
            .map(|index| Predicate::truthy_path(format!("ambient{index}")))
            .collect(),
    );
    let mut union = arena.decision(ambient.clone(), ambient.negated(), Coverage::Exact);
    let mut widened = false;
    for index in 0..160 {
        let condition = Predicate::from(helm_schema_core::Guard::Eq {
            path: helm_schema_core::ValuesPath::parse("branch"),
            value: GuardValue::Int(index),
        });
        let atom = arena.decision(condition.clone(), condition.negated(), Coverage::Exact);
        union = arena.surviving_decision(
            &[DecisionEdge {
                test: union,
                taken: true,
            }],
            &[DecisionEdge {
                test: Rc::clone(&atom),
                taken: true,
            }],
        );
        widened |= union.coverage == Coverage::Partial;
        assert!(
            union
                .runtime
                .as_ref()
                .is_some_and(RuntimeGuard::is_input_exact)
        );
        let selected = arena.runtime_path(&[DecisionEdge {
            test: atom,
            taken: true,
        }]);
        sim_assert_eq!(have: selected.implies_test(&union), want: Some(true));
    }
    assert!(widened, "the fixture must exhaust exported projection work");
}

#[test]
fn growing_branch_chains_keep_one_normal_continuation() -> eyre::Result<()> {
    helm_schema_test_support::init_perfetto_from_env()?;
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for count in [1, 2, 4, 8, 16, 32] {
        let span = tracing::info_span!("branch_chain", count);
        let _entered = span.enter();
        let mut source = String::from("{{ $value := float64 .Values.input }}");
        for index in 0..count {
            let keyword = if index == 0 { "if" } else { "else if" };
            write!(
                source,
                "{{{{ {keyword} eq .Values.branch {index} }}}}{{{{ $value = float64 .Values.input }}}}"
            )?;
        }
        source.push_str("{{ end }}{{ $value }}");
        let execution = crate::value::tests::execute_document(&source, None, &db);
        sim_assert_eq!(have: execution.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
    }
    Ok(())
}

#[test]
fn three_surviving_paths_reconverge_in_every_order_without_losing_correlation() -> eyre::Result<()>
{
    let mut initial = State::new(Value::nil());
    let map = initial.heap.map(Remainder::Closed);
    let target = initial.map_target(&map).ok_or_eyre("map missing")?;
    initial.bind("$map".to_owned(), map.clone());
    let atoms: [_; 3] = std::array::from_fn(|_| {
        initial
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial)
    });
    let [branch, left, right] = &atoms;
    let mut states = [
        initial
            .fork(Rc::clone(branch), true)
            .fork(Rc::clone(left), true),
        initial
            .fork(Rc::clone(branch), false)
            .fork(Rc::clone(right), true),
        initial
            .fork(Rc::clone(branch), false)
            .fork(Rc::clone(right), false),
    ];
    for (index, state) in states.iter_mut().enumerate() {
        state.heap.write(
            &target,
            "value",
            Field::Present(Value::string(&index.to_string())),
        );
    }
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let exits = order
            .into_iter()
            .map(|index| Exit {
                state: states[index].clone(),
                flow: Flow::Normal(Normal {
                    value: Value::string(&index.to_string()),
                }),
            })
            .collect();
        let joined = crate::value::control::reconverge(exits);
        sim_assert_eq!(have: joined.len(), want: 1);
        let exit = joined.first().ok_or_eyre("normal continuation missing")?;
        let Flow::Normal(normal) = &exit.flow else {
            eyre::bail!("continuation is terminal");
        };
        for assignment in 0_u8..8 {
            let mut selected = exit.state.clone();
            Rc::make_mut(&mut selected.active).extend(atoms.iter().enumerate().map(
                |(index, test)| DecisionEdge {
                    test: Rc::clone(test),
                    taken: assignment & (1 << index) != 0,
                },
            ));
            let possible = assignment & 1 == 0 || assignment & 2 != 0;
            sim_assert_eq!(have: selected.path_predicate() != Predicate::False, want: possible);
            if possible {
                let expected = if assignment & 1 != 0 {
                    "0"
                } else if assignment & 4 != 0 {
                    "1"
                } else {
                    "2"
                };
                sim_assert_eq!(have: selected.consumed(&normal.value).constant().cloned(), want: Some(GuardValue::String(expected.to_owned())));
                let field = selected.field(&map, "value");
                sim_assert_eq!(have: selected.consumed(&field).constant().cloned(), want: Some(GuardValue::String(expected.to_owned())));
            }
        }
    }
    Ok(())
}

#[test]
fn overlapping_paths_preserve_both_outcomes_only_in_their_original_domains() -> eyre::Result<()> {
    let initial = State::new(Value::nil());
    let atoms: [_; 2] = std::array::from_fn(|_| {
        initial
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial)
    });
    let exits = atoms
        .iter()
        .enumerate()
        .map(|(index, test)| Exit {
            state: initial.fork(Rc::clone(test), true),
            flow: Flow::Normal(Normal {
                value: Value::string(&index.to_string()),
            }),
        })
        .collect();
    let exits = crate::value::control::reconverge(exits);
    sim_assert_eq!(have: exits.len(), want: 1);
    let exit = exits.first().ok_or_eyre("normal continuation missing")?;
    let Flow::Normal(normal) = &exit.flow else {
        eyre::bail!("continuation is terminal");
    };
    for assignment in 0_u8..4 {
        let mut selected = exit.state.clone();
        Rc::make_mut(&mut selected.active).extend(atoms.iter().enumerate().map(|(index, test)| {
            DecisionEdge {
                test: Rc::clone(test),
                taken: assignment & (1 << index) != 0,
            }
        }));
        sim_assert_eq!(have: selected.path_predicate() != Predicate::False, want: assignment != 0);
        if assignment == 0 {
            continue;
        }
        let value = selected.consumed(&normal.value);
        let mut pending = vec![&value];
        let mut outcomes = std::collections::BTreeSet::new();
        while let Some(value) = pending.pop() {
            match value.node.as_ref() {
                ValueNode::Leaf(crate::value::Leaf {
                    datum: _,
                    origin: _,
                }) => {
                    outcomes.insert(value.constant().cloned());
                }
                ValueNode::Select {
                    decision: _,
                    yes,
                    no,
                } => {
                    pending.extend([yes, no]);
                }
            }
        }
        let expected = (0..2)
            .filter(|index| assignment & (1 << index) != 0)
            .map(|index| Some(GuardValue::String(index.to_string())))
            .collect();
        sim_assert_eq!(have: outcomes, want: expected);
    }
    Ok(())
}

#[test]
fn template_identity_is_shared_within_one_file_and_fresh_for_the_next() -> eyre::Result<()> {
    use crate::value::session::{FileOccurrence, begin_file};

    let mut initial = State::new(Value::nil());
    let root = initial.heap.map(Remainder::Closed);
    let decision =
        initial
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial);
    let occurrence = FileOccurrence::new(&initial);
    let mut targets = Vec::new();
    for taken in [true, false] {
        let mut branch = initial.fork(Rc::clone(&decision), taken);
        begin_file(
            &mut branch,
            &root,
            Some("chart/templates/z.yaml"),
            &occurrence,
        );
        let template = branch.field(&root, "Template");
        let target = branch
            .map_target(&template)
            .ok_or_eyre("template missing")?;
        targets.push(target.clone());
        let next = FileOccurrence::new(&branch);
        begin_file(&mut branch, &root, Some("chart/templates/a.yaml"), &next);
        let replacement = branch.field(&root, "Template");
        assert!(
            branch
                .map_target(&replacement)
                .is_some_and(|next| next != target)
        );
        let previous_name = branch.field(&template, "Name");
        sim_assert_eq!(have: previous_name.constant().cloned(), want: Some(GuardValue::String("chart/templates/z.yaml".to_owned())));
    }
    sim_assert_eq!(have: targets.first(), want: targets.last());
    Ok(())
}

#[test]
fn independent_helper_regions_and_files_have_one_normal_continuation() {
    let mut defines = DefineIndex::new();
    defines.add_file_source("chart/templates/_helpers.tpl", indoc! {r#"
        {{- define "vpa.name" -}}
        {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
        {{- end }}
        {{- define "vpa.chart" -}}
        {{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
        {{- end }}
        {{- define "vpa.labels" -}}
        helm.sh/chart: {{ include "vpa.chart" . }}
        {{ include "vpa.selectorLabels" . }}
        {{- if .Chart.AppVersion }}
        app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
        {{- end }}
        app.kubernetes.io/managed-by: {{ .Release.Service }}
        {{- if .Values.podLabels }}
        {{ toYaml .Values.podLabels }}
        {{- end }}
        {{- end }}
        {{- define "vpa.selectorLabels" -}}
        app.kubernetes.io/name: {{ include "vpa.name" . }}
        app.kubernetes.io/instance: {{ .Release.Name }}
        {{- end }}
    "#});
    let db = IrAnalysisDb::new(&defines);
    for count in [1, 2, 4, 8] {
        let owned: Vec<_> = (0..count)
            .map(|index| {
                // The first file is the profiler's VPA reproducer; later files add
                // independent branches to check that continuations do not multiply.
                let condition = if index == 0 {
                    "admissionController.enabled".to_owned()
                } else {
                    format!("enabled{index}")
                };
                (
                    formatdoc! {r#"
                {{{{- if .Values.{condition} }}}}
                x:
                  {{{{- include "vpa.labels" . | nindent 4 }}}}
                {{{{- end }}}}
            "#},
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
        for document in execute_documents(&sources, &db) {
            sim_assert_eq!(have: document.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
        }
    }
}

struct StorageGrowth {
    nodes: usize,
    payload: usize,
}

fn storage_growth(version: &crate::value::HeapVersion) -> StorageGrowth {
    use crate::value::{AliasSet, HeapNode, ObjectData};
    let mut pending = vec![version];
    let mut nodes = std::collections::BTreeSet::new();
    let mut objects = std::collections::BTreeSet::new();
    let mut inputs = std::collections::BTreeSet::new();
    let mut payload = 0;
    while let Some(version) = pending.pop() {
        if !nodes.insert(version.id.ordinal) {
            continue;
        }
        match &version.node {
            HeapNode::Empty => {}
            HeapNode::Input { parent } => pending.push(parent),
            HeapNode::Install {
                parent,
                object: _,
                storage,
            } => {
                pending.push(parent);
                if !objects.insert(Rc::as_ptr(storage)) {
                    continue;
                }
                match &storage.data {
                    ObjectData::Map {
                        fields,
                        remainder: _,
                    } => payload += fields.len(),
                    ObjectData::Strings(slots) => payload += slots.len(),
                    ObjectData::Opaque { reachable: _ } => payload += 1,
                    ObjectData::Widened { inputs: known } => {
                        if inputs.insert(Rc::as_ptr(known)) {
                            payload += known.len();
                        }
                    }
                }
            }
            HeapNode::Write {
                parent,
                object: _,
                key: _,
                field: _,
            }
            | HeapNode::WriteSlot {
                parent,
                object: _,
                index: _,
                value: _,
            } => {
                pending.push(parent);
                payload += 1;
            }
            HeapNode::UnknownWrite {
                parent,
                objects,
                inputs: known,
            } => {
                pending.push(parent);
                if inputs.insert(Rc::as_ptr(known)) {
                    payload += known.len();
                }
                payload += match objects {
                    AliasSet::Known(objects) => objects.len(),
                    AliasSet::RenderGraph => 1,
                };
            }
            HeapNode::Join {
                decision: _,
                yes,
                no,
            } => pending.extend([yes.as_ref(), no.as_ref()]),
        }
    }
    StorageGrowth {
        nodes: nodes.len(),
        payload,
    }
}

#[test]
fn successive_unknown_templates_keep_one_normal_continuation() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    for count in [1, 2, 4, 8, 16, 24] {
        let mut source = String::new();
        for index in 0..count {
            write!(source, "{{{{ $_ := tpl .Values.program{index} . }}}}")?;
        }
        let execution = crate::value::tests::execute_document(&source, None, &db);
        sim_assert_eq!(have: execution.exits.iter().filter(|exit| matches!(exit.flow, Flow::Normal(_))).count(), want: 1);
        let normal = execution
            .exits
            .iter()
            .find(|exit| matches!(exit.flow, Flow::Normal(_)))
            .ok_or_eyre("normal continuation missing")?;
        let growth = storage_growth(&normal.state.heap.version);
        assert!(
            growth.nodes <= 32 * count + 64,
            "{count} calls retain {} nodes",
            growth.nodes
        );
        assert!(
            growth.payload <= 32 * count + 64,
            "{count} calls retain {} payload entries",
            growth.payload
        );
    }
    Ok(())
}

#[test]
fn heap_query_exhaustion_never_reuses_a_cached_freshness_proof() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .write(&object, "x", Field::Present(Value::string("before")));
    let _precise = state
        .heap
        .version
        .object(&object)
        .ok_or_eyre("object missing")?;
    let exhausted = state
        .heap
        .version
        .object_with_budget(&object, 0)
        .ok_or_eyre("exhaustion dropped the object")?;
    sim_assert_eq!(have: exhausted.multiplicity, want: crate::value::Multiplicity::Summary);
    assert!(matches!(
        exhausted.data,
        crate::value::ObjectData::Widened { inputs: _ }
    ));
    sim_assert_eq!(have: state.heap.version.field(&object, "x"), want: Field::Present(Value::string("before")));
    Ok(())
}

#[test]
fn frozen_heap_queries_survive_the_live_session_owner() -> eyre::Result<()> {
    let frozen = {
        let mut state = State::new(Value::nil());
        let map = state.heap.map(Remainder::Closed);
        let object = state.map_target(&map).ok_or_eyre("map missing")?;
        state
            .heap
            .write(&object, "x", Field::Present(Value::string("before")));
        state.heap.freeze(&map)
    };
    let field = frozen
        .read_field("x")
        .present()
        .ok_or_eyre("frozen field missing")?;
    sim_assert_eq!(have: field.exact_text(), want: Some("before".to_owned()));
    Ok(())
}

#[test]
fn borrowed_heap_views_pin_fields_after_cache_eviction_and_later_writes() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .write(&object, "x", Field::Present(Value::string("before")));
    let frozen = state.heap.freeze(&map);
    let borrowed = frozen
        .heap
        .borrowed_object(&object)
        .ok_or_eyre("borrowed view missing")?;

    // The frozen root pins its derived fields independently of the evictable lookup cache.
    *state.heap.arena.heap_queries.borrow_mut() = crate::value::heap_cache::ObjectCache::default();
    state
        .heap
        .write(&object, "x", Field::Present(Value::string("after")));
    drop(state);
    let again = frozen
        .heap
        .borrowed_object(&object)
        .ok_or_eyre("pinned view missing")?;
    sim_assert_eq!(have: std::ptr::eq(borrowed, again), want: true);
    let crate::value::ObjectData::Map {
        fields,
        remainder: _,
    } = &borrowed.data
    else {
        eyre::bail!("expected map storage");
    };
    sim_assert_eq!(have: fields.get("x"), want: Some(&Field::Present(Value::string("before"))));
    Ok(())
}

#[test]
fn exhausted_provenance_queries_do_not_publish_partial_cache_entries() -> eyre::Result<()> {
    use crate::value::ObjectData;
    use helm_schema_core::ValuesPath;
    use std::collections::BTreeSet;

    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .unknown_write(Rc::new(BTreeSet::from([ValuesPath::parse("x")])));
    state.heap.write(
        &object,
        "next",
        Field::Present(Value::input(ValuesPath::parse("y"))),
    );
    let partial = state
        .heap
        .version
        .object_with_budget(&object, 4)
        .ok_or_eyre("exhaustion dropped storage")?;
    let ObjectData::Widened { inputs } = &partial.data else {
        eyre::bail!("exhaustion established precision");
    };
    assert!(inputs.contains(&ValuesPath::default()));
    let complete = state
        .heap
        .version
        .object(&object)
        .ok_or_eyre("storage missing")?;
    // A completed query preserves the known last write and its unknown remainder.
    sim_assert_eq!(
        have: &complete.data,
        want: &ObjectData::Map {
            fields: Rc::new(std::collections::BTreeMap::from([(
                "next".to_owned(),
                Field::Present(Value::input(ValuesPath::parse("y"))),
            )])),
            remainder: Remainder::Unknown {
                inputs: Rc::new(BTreeSet::from([ValuesPath::parse("x")])),
            },
        }
    );
    Ok(())
}

#[test]
fn frozen_object_enumeration_reports_budget_exhaustion() -> eyre::Result<()> {
    let frozen = {
        let mut state = State::new(Value::nil());
        let map = state.heap.map(Remainder::Closed);
        state.heap.freeze(&map)
    };
    let mut budget = crate::value::heap_query::QueryBudget::new(0);
    sim_assert_eq!(have: frozen.heap.query_object_ids(&mut budget), want: None);
    sim_assert_eq!(have: frozen.heap.object_ids().ok_or_eyre("complete roster missing")?.len(), want: 1);
    Ok(())
}

#[test]
fn reconvergence_preserves_coverage_for_widened_objects_never_read_again() -> eyre::Result<()> {
    use crate::value::ReadCoverage;
    use helm_schema_core::ValuesPath;

    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let object = state.map_target(&map).ok_or_eyre("map identity missing")?;
    let mut heap = state.heap;
    let mut yes = heap.fork();
    let mut no = heap.fork();
    for index in 0..65 {
        yes.write(
            &object,
            &format!("a{index}"),
            Field::Present(Value::input(ValuesPath::parse("a"))),
        );
        no.write(
            &object,
            &format!("b{index}"),
            Field::Present(Value::input(ValuesPath::parse("b"))),
        );
    }
    let predicate = Predicate::truthy_path("choose");
    let decision = heap
        .arena
        .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    heap.join(&decision, &yes, &no)?;
    let mut reads = crate::value::read_set::ReadSet::default();
    reads.observe_heap(Rc::clone(&heap.version), Predicate::True);
    let regions: Vec<_> = reads
        .iter()
        .filter(|read| read.coverage == ReadCoverage::UnknownRegion)
        .map(|read| (read.path.clone(), read.condition.clone()))
        .collect();
    sim_assert_eq!(have: regions, want: vec![(ValuesPath::parse("a"), Predicate::True), (ValuesPath::parse("b"), Predicate::True)]);
    Ok(())
}

#[test]
fn heap_observations_freeze_input_identity_when_a_summary_object_is_reused() {
    use crate::value::{Multiplicity, ObjectData};
    use helm_schema_core::ValuesPath;

    let mut state = State::new(Value::nil());
    let object = state.heap.allocate(
        ObjectData::Widened {
            inputs: Rc::default(),
        },
        Multiplicity::Summary,
    );
    state
        .heap
        .register_input(object.clone(), ValuesPath::parse("x"));
    let before = Rc::clone(&state.heap.version);
    state.heap.register_input(object, ValuesPath::parse("y"));
    let mut reads = crate::value::read_set::ReadSet::default();
    reads.observe_heap(before, Predicate::truthy_path("before"));
    reads.observe_heap(
        Rc::clone(&state.heap.version),
        Predicate::truthy_path("after"),
    );
    let regions: Vec<_> = reads
        .iter()
        .map(|read| (read.path.clone(), read.condition.clone()))
        .collect();
    sim_assert_eq!(have: regions, want: vec![(ValuesPath::parse("x"), Predicate::truthy_path("before")), (ValuesPath::parse("y"), Predicate::truthy_path("after"))]);
}

#[test]
fn coverage_cache_keys_keep_independent_sessions_separate() {
    use crate::value::{Multiplicity, ObjectData};
    use helm_schema_core::ValuesPath;
    use std::collections::BTreeSet;

    let mut reads = crate::value::read_set::ReadSet::default();
    let mut owners = Vec::new();
    for path in ["a", "b"] {
        let mut state = State::new(Value::nil());
        state.heap.allocate(
            ObjectData::Widened {
                inputs: Rc::new(BTreeSet::from([ValuesPath::parse(path)])),
            },
            Multiplicity::Singleton,
        );
        reads.observe_heap(Rc::clone(&state.heap.version), Predicate::True);
        owners.push(state);
    }
    let regions: Vec<_> = reads.iter().map(|read| read.path.clone()).collect();
    sim_assert_eq!(have: regions, want: vec![ValuesPath::parse("a"), ValuesPath::parse("b")]);
}

#[test]
fn unrelated_history_does_not_spend_a_members_query_budget() -> eyre::Result<()> {
    use crate::value::ObjectData;
    use std::collections::BTreeMap;

    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .write(&target, "x", Field::Present(Value::string("before")));
    let before = state.heap.freeze(&map);
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    for _ in 0..512 {
        let temporary = state.heap.map(Remainder::Closed);
        let object = state
            .map_target(&temporary)
            .ok_or_eyre("temporary map missing")?;
        let mut yes = state.heap.fork();
        let no = state.heap.fork();
        yes.write(&object, "other", Field::Present(Value::string("value")));
        state.heap.join(&decision, &yes, &no)?;
    }
    let stored = state
        .heap
        .version
        .object_with_budget(&target, 64)
        .ok_or_eyre("stored map missing")?;
    sim_assert_eq!(have: &stored.data, want: &ObjectData::Map { fields: Rc::new(BTreeMap::from([("x".to_owned(), Field::Present(Value::string("before")))])), remainder: Remainder::Closed });
    state
        .heap
        .write(&target, "x", Field::Present(Value::string("after")));
    let stored = state
        .heap
        .version
        .object_with_budget(&target, 64)
        .ok_or_eyre("written map missing")?;
    sim_assert_eq!(have: &stored.data, want: &ObjectData::Map { fields: Rc::new(BTreeMap::from([("x".to_owned(), Field::Present(Value::string("after")))])), remainder: Remainder::Closed });
    sim_assert_eq!(have: before.read_field("x").present().ok_or_eyre("frozen member missing")?.exact_text(), want: Some("before".to_owned()));
    Ok(())
}

#[test]
fn allocation_counts_do_not_mistake_an_implicit_input_view_for_installed_storage()
-> eyre::Result<()> {
    use crate::value::{
        AliasSet, Datum, Leaf, MapType, Multiplicity, ObjectData, ObjectVersion, Origin,
        TypeCoverage, UnknownValue,
    };
    let mut state = State::new(Value::nil());
    state
        .map_target(&Value::input(helm_schema_core::ValuesPath::default()))
        .ok_or_eyre("input root missing")?;
    let input = Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Unknown(UnknownValue {
                types: TypeCoverage::Map(MapType::Interface),
                aliases: AliasSet::RenderGraph,
            }),
            origin: Origin {
                derivation: None,
                identity: Some(helm_schema_core::ValuesPath::parse("x")),
                influences: std::collections::BTreeSet::default(),
            },
        })),
    };
    let mut discarded = state.clone();
    let object = discarded
        .map_target(&input)
        .ok_or_eyre("input object missing")?;
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    yes.map(Remainder::Closed);
    no.map(Remainder::Closed);
    let decision = state
        .heap
        .arena
        .decision(Predicate::False, Predicate::False, Coverage::Partial);
    state.heap.join(&decision, &yes, &no)?;
    let before = state.heap.version.object_count();
    assert!(state.heap.version.object(&object).is_none());
    state.heap.install(
        object,
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Map {
                fields: Rc::new(std::collections::BTreeMap::default()),
                remainder: Remainder::Input,
            },
        },
    );
    sim_assert_eq!(have: state.heap.version.object_count(), want: before + 1);
    sim_assert_eq!(have: state.heap.version.object_count(), want: state.heap.version.object_ids().expect("bounded test object roster").len());
    Ok(())
}

#[test]
fn unknown_write_queries_do_not_reconstruct_discarded_storage() -> eyre::Result<()> {
    use crate::value::{Multiplicity, ObjectData, ObjectVersion};
    use helm_schema_core::ValuesPath;
    use std::collections::{BTreeMap, BTreeSet};

    let mut state = State::new(Value::nil());
    let fields = (0..63)
        .map(|index| (index.to_string(), Field::Present(Value::string("value"))))
        .collect::<BTreeMap<_, _>>();
    let target = state.heap.allocate(
        ObjectData::Map {
            fields: Rc::new(fields),
            remainder: Remainder::Closed,
        },
        Multiplicity::Singleton,
    );
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    yes.write(&target, "yes", Field::Present(Value::string("yes")));
    no.write(&target, "no", Field::Present(Value::string("no")));
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    let inputs = Rc::new(BTreeSet::from([ValuesPath::parse("effect")]));
    state.heap.unknown_write(Rc::clone(&inputs));

    // Havoc needs the object's identity and multiplicity, but discards the joined fields.
    let object = state
        .heap
        .version
        .object_with_budget(&target, 64)
        .ok_or_eyre("object missing")?;
    sim_assert_eq!(have: object.as_ref(), want: &ObjectVersion {
        revision: state.heap.version.id,
        multiplicity: Multiplicity::Singleton,
        data: ObjectData::Widened { inputs },
    });
    Ok(())
}

#[test]
fn registering_input_identity_does_not_create_a_new_storage_revision() -> eyre::Result<()> {
    use helm_schema_core::ValuesPath;

    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    yes.write(&target, "value", Field::Present(Value::string("yes")));
    no.write(&target, "value", Field::Present(Value::string("no")));
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    let before = state
        .heap
        .version
        .object(&target)
        .ok_or_eyre("joined map missing")?;

    // Discovering another input changes frozen identity metadata without writing this map.
    let other = state.heap.arena.object();
    state.heap.register_input(other, ValuesPath::parse("other"));
    let after = state
        .heap
        .version
        .object(&target)
        .ok_or_eyre("joined map disappeared")?;
    sim_assert_eq!(have: after.as_ref(), want: before.as_ref());
    Ok(())
}

#[test]
fn joined_frozen_yaml_can_query_another_join_without_reentering_the_union_cache() -> eyre::Result<()>
{
    use crate::value::{ObjectData, text::TextTerm};
    use std::collections::{BTreeMap, BTreeSet};

    let mut state = State::new(Value::nil());
    let inner = state.heap.map(Remainder::Closed);
    let inner_id = state.map_target(&inner).ok_or_eyre("inner map missing")?;
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    yes.write(&inner_id, "key", Field::Present(Value::string("yes")));
    no.write(&inner_id, "key", Field::Present(Value::string("no")));
    let predicate = Predicate::truthy_path("inner");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    let yaml = Value::frozen_text(
        TextTerm::Yaml {
            value: state.heap.freeze(&inner),
            pretty: false,
        },
        BTreeSet::new(),
    );

    let outer = state.heap.map(Remainder::Closed);
    let outer_id = state.map_target(&outer).ok_or_eyre("outer map missing")?;
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    yes.write(&outer_id, "value", Field::Present(yaml.clone()));
    no.write(&outer_id, "value", Field::Present(Value::string("safe")));
    let predicate = Predicate::truthy_path("outer");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;

    // Selecting the YAML value queries the still-uncached inner heap join.
    let object = state
        .heap
        .version
        .object(&outer_id)
        .ok_or_eyre("joined outer map missing")?;
    sim_assert_eq!(have: &object.data, want: &ObjectData::Map {
        fields: Rc::new(BTreeMap::from([("value".to_owned(), Field::select(decision, Field::Present(yaml), Field::Present(Value::string("safe"))))])),
        remainder: Remainder::Closed,
    });
    Ok(())
}

#[test]
fn seeded_object_inputs_follow_unknown_aliases_and_cycles_without_a_live_arena() -> eyre::Result<()>
{
    use crate::value::heap_query::QueryBudget;
    use crate::value::{AliasSet, Datum, FrozenValue, TypeCoverage, UnknownValue};
    use helm_schema_core::ValuesPath;
    use std::collections::BTreeSet;

    let mut state = State::new(Value::nil());
    let root = state.heap.map(Remainder::Closed);
    let root_id = state.map_target(&root).ok_or_eyre("root missing")?;
    let child = state.heap.map(Remainder::Closed);
    let child_id = state.map_target(&child).ok_or_eyre("child missing")?;
    let other = state.heap.map(Remainder::Closed);
    let other_id = state.map_target(&other).ok_or_eyre("other map missing")?;
    for (target, path) in [(&child_id, "source"), (&other_id, "unreachable")] {
        state.heap.write(
            target,
            "value",
            Field::Present(Value::with_influences(
                Value::string("constant"),
                BTreeSet::from([ValuesPath::parse(path)]),
            )),
        );
    }
    state.heap.write(&child_id, "cycle", Field::Present(root));
    state.heap.write(
        &root_id,
        "alias",
        Field::Present(Value::leaf(Datum::Unknown(UnknownValue {
            types: TypeCoverage::Unresolved,
            aliases: AliasSet::Known(Rc::new(BTreeSet::from([child_id]))),
        }))),
    );
    let storage = state
        .heap
        .version
        .object(&root_id)
        .ok_or_eyre("stored root missing")?;
    let heap = Rc::clone(&state.heap.version);
    drop(state);
    sim_assert_eq!(have: FrozenValue::object_inputs(&heap, &root_id, &storage, &mut QueryBudget::new(65_536)), want: BTreeSet::from([ValuesPath::parse("source")]));
    sim_assert_eq!(have: FrozenValue::object_inputs(&heap, &root_id, &storage, &mut QueryBudget::new(0)), want: BTreeSet::from([ValuesPath::default()]));
    Ok(())
}

#[test]
fn an_absent_join_arm_does_not_spend_the_other_arms_query_budget() -> eyre::Result<()> {
    use crate::value::{Heap, ObjectData};
    use std::collections::{BTreeMap, BTreeSet};

    let mut heap = Heap::new();
    let mut yes = heap.fork();
    let mut no = heap.fork();
    let value = yes.map(Remainder::Closed);
    let mut state = State::with_heap(Value::nil(), yes);
    let target = state.map_target(&value).ok_or_eyre("target missing")?;
    state
        .heap
        .write(&target, "saved", Field::Present(Value::string("before")));
    yes = state.heap;
    for _ in 0..128 {
        let mut writes = no.fork();
        writes.map(Remainder::Closed);
        writes.unknown_write(Rc::new(BTreeSet::new()));
        let decision = no
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial);
        let unchanged = no.fork();
        no.join(&decision, &writes, &unchanged)?;
    }
    let decision = heap
        .arena
        .decision(Predicate::False, Predicate::False, Coverage::Partial);
    heap.join(&decision, &yes, &no)?;
    let storage = heap
        .version
        .object_with_budget(&target, 64)
        .ok_or_eyre("joined object missing")?;
    sim_assert_eq!(have: &storage.data, want: &ObjectData::Map {
        fields: Rc::new(BTreeMap::from([("saved".to_owned(), Field::Present(Value::string("before")))])),
        remainder: Remainder::Closed,
    });
    Ok(())
}

#[test]
fn provenance_memo_separates_snapshots_and_alias_modes_and_never_caches_exhaustion()
-> eyre::Result<()> {
    use crate::value::heap_query::QueryBudget;
    use crate::value::{AliasSet, Datum, TypeCoverage, UnknownValue};
    use helm_schema_core::ValuesPath;
    use std::collections::BTreeSet;

    let mut state = State::new(Value::nil());
    let root = state.heap.map(Remainder::Closed);
    let root_id = state.map_target(&root).ok_or_eyre("root missing")?;
    let child = state.heap.map(Remainder::Closed);
    let child_id = state.map_target(&child).ok_or_eyre("child missing")?;
    state.heap.write(
        &child_id,
        "value",
        Field::Present(Value::with_influences(
            Value::string("constant"),
            BTreeSet::from([ValuesPath::parse("before")]),
        )),
    );
    state.heap.write(
        &root_id,
        "alias",
        Field::Present(Value::leaf(Datum::Unknown(UnknownValue {
            types: TypeCoverage::Unresolved,
            aliases: AliasSet::Known(Rc::new(BTreeSet::from([child_id.clone()]))),
        }))),
    );
    let old = state.heap.freeze(&root);
    sim_assert_eq!(have: old.collect_inputs_with_budget(true, &mut QueryBudget::new(2)), want: BTreeSet::from([ValuesPath::default()]));
    sim_assert_eq!(have: old.inputs(), want: BTreeSet::new());
    sim_assert_eq!(have: old.reachable_inputs(), want: BTreeSet::from([ValuesPath::parse("before")]));
    sim_assert_eq!(have: old.collect_inputs_with_budget(true, &mut QueryBudget::new(0)), want: BTreeSet::from([ValuesPath::default()]));
    state.heap.write(
        &child_id,
        "value",
        Field::Present(Value::with_influences(
            Value::string("constant"),
            BTreeSet::from([ValuesPath::parse("after")]),
        )),
    );
    let new = state.heap.freeze(&root);
    sim_assert_eq!(have: new.reachable_inputs(), want: BTreeSet::from([ValuesPath::parse("after")]));
    sim_assert_eq!(have: old.reachable_inputs(), want: BTreeSet::from([ValuesPath::parse("before")]));
    *state.heap.arena.input_queries.borrow_mut() = crate::value::heap_cache::InputCache::default();
    sim_assert_eq!(have: old.reachable_inputs(), want: BTreeSet::from([ValuesPath::parse("before")]));
    let arena = Rc::downgrade(&state.heap.arena);
    drop(state);
    assert!(arena.upgrade().is_none());
    sim_assert_eq!(have: old.reachable_inputs(), want: BTreeSet::from([ValuesPath::parse("before")]));
    Ok(())
}

#[test]
fn terminal_object_provenance_keeps_exact_work_and_does_not_invent_input_identity()
-> eyre::Result<()> {
    use std::collections::{BTreeMap, BTreeSet};

    use helm_schema_core::ValuesPath;

    use crate::value::heap_query::QueryBudget;
    use crate::value::{AliasSet, FrozenValue, Multiplicity, ObjectData};

    struct Case {
        data: ObjectData,
        inputs: BTreeSet<ValuesPath>,
        work: usize,
    }
    let inputs = BTreeSet::from([ValuesPath::parse("left"), ValuesPath::parse("right")]);
    let mut cases = [Remainder::Closed, Remainder::Open, Remainder::Input]
        .into_iter()
        .map(|remainder| Case {
            data: ObjectData::Map {
                fields: Rc::new(BTreeMap::new()),
                remainder,
            },
            inputs: BTreeSet::new(),
            work: 2,
        })
        .collect::<Vec<_>>();
    cases.extend([
        Case {
            data: ObjectData::Map {
                fields: Rc::new(BTreeMap::new()),
                remainder: Remainder::Unknown {
                    inputs: Rc::new(inputs.clone()),
                },
            },
            inputs: inputs.clone(),
            work: 2,
        },
        Case {
            data: ObjectData::Strings(Vec::new()),
            inputs: BTreeSet::new(),
            work: 2,
        },
        Case {
            data: ObjectData::Opaque {
                reachable: AliasSet::Known(Rc::new(BTreeSet::new())),
            },
            inputs: BTreeSet::new(),
            work: 2,
        },
        Case {
            data: ObjectData::Widened {
                inputs: Rc::new(inputs.clone()),
            },
            inputs,
            work: 4,
        },
    ]);
    for Case { data, inputs, work } in cases {
        let mut state = State::new(Value::nil());
        let object = state.heap.allocate(data, Multiplicity::Singleton);
        state
            .heap
            .register_input(object.clone(), ValuesPath::parse("identityOnly"));
        let heap = Rc::clone(&state.heap.version);
        let storage = heap.object(&object).ok_or_eyre("object missing")?;
        drop(state);
        for allowance in 0..=work {
            let mut budget = QueryBudget::new(allowance);
            sim_assert_eq!(have: FrozenValue::object_inputs(&heap, &object, &storage, &mut budget), want: BTreeSet::from([ValuesPath::default()]));
            sim_assert_eq!(have: budget.is_exhausted(), want: true);
        }
        let mut budget = QueryBudget::new(work + 1);
        sim_assert_eq!(have: FrozenValue::object_inputs(&heap, &object, &storage, &mut budget), want: inputs);
        sim_assert_eq!(have: budget.remaining, want: 1);
        sim_assert_eq!(have: budget.is_exhausted(), want: false);
    }
    Ok(())
}

#[test]
fn metadata_reuse_distinguishes_absence_retargeting_and_shared_prefixes() {
    use helm_schema_core::ValuesPath;

    use crate::value::ObjectId;
    use crate::value::input_paths::InputPaths;

    let mut paths = InputPaths::default();
    let mut versions = vec![paths.clone()];
    for (ordinal, path) in [
        (0, "root"),
        (1, "one"),
        (7, "seven"),
        (64, "large"),
        (1 << 40, "sparse"),
        (1, "replacement"),
        (1, "one"),
        (u64::MAX, "last"),
        (0, ""),
    ] {
        paths.insert(ObjectId { ordinal }, ValuesPath::parse(path));
        versions.push(paths.clone());
    }
    for left in &versions {
        for right in &versions {
            for ordinal in [0, 1, 2, 7, 8, 63, 64, 65, 1 << 40, u64::MAX] {
                let object = ObjectId { ordinal };
                sim_assert_eq!(have: left.same_mapping(right, &object), want: left.get(&object) == right.get(&object));
            }
        }
    }
}
