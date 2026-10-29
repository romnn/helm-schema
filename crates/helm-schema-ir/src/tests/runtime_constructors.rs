use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;
use tracing_subscriber::prelude::*;

use crate::analysis_db::IrAnalysisDb;
use crate::value::eval::Executor;
use crate::value::{
    ArgumentForm, Callee, CompletionInput, Coverage, Exit, Flow, FrozenTest, Invocation, Operand,
    State, Value,
};

#[derive(Debug, Default, PartialEq, Eq)]
struct Work {
    alternatives: usize,
    forks: usize,
    allocations: usize,
    helpers: usize,
    exits: usize,
}

#[derive(Default)]
struct Counters {
    alternatives: AtomicUsize,
    forks: AtomicUsize,
    allocations: AtomicUsize,
    helpers: AtomicUsize,
}

struct WorkLayer(Arc<Counters>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for WorkLayer {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let Self(counters) = self;
        let metadata = attributes.metadata();
        let counter = match metadata.name() {
            "transfer_invocation" => &counters.alternatives,
            "fork" if metadata.module_path() == Some("helm_schema_ir::value::state") => {
                &counters.forks
            }
            "allocate" => &counters.allocations,
            "helper" => &counters.helpers,
            _ => return,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

struct Measured {
    exits: Vec<Exit>,
    work: Work,
}

fn measure(run: impl FnOnce() -> Vec<Exit>) -> Measured {
    let counters = Arc::new(Counters::default());
    let subscriber = tracing_subscriber::registry().with(WorkLayer(Arc::clone(&counters)));
    let exits = tracing::subscriber::with_default(subscriber, run);
    let work = Work {
        alternatives: counters.alternatives.load(Ordering::Relaxed),
        forks: counters.forks.load(Ordering::Relaxed),
        allocations: counters.allocations.load(Ordering::Relaxed),
        helpers: counters.helpers.load(Ordering::Relaxed),
        exits: exits.len(),
    };
    Measured { exits, work }
}

fn invoke(executor: &Executor<'_>, function: &str, values: Vec<Value>, state: State) -> Vec<Exit> {
    executor
        .complete(CompletionInput::Invocation {
            invocation: Invocation {
                callee: Callee::Function(function.to_owned()),
                operands: values
                    .into_iter()
                    .map(|value| Operand {
                        value,
                        site: 0,
                        form: ArgumentForm::Evaluated,
                    })
                    .collect(),
                site: 0,
            },
            state,
        })
        .into_exits()
}

fn decision(state: &State) -> Rc<FrozenTest> {
    let predicate = Predicate::from(Guard::Truthy {
        path: ValuesPath::parse("choice"),
    });
    state
        .heap
        .arena
        .decision(predicate.clone(), predicate.negated(), Coverage::Exact)
}

fn correlated_constructor(function: &str) {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    let mut measured = Vec::new();
    for count in [1, 2, 4, 8, 16] {
        let state = State::new(Value::nil());
        let choice = Value::select(decision(&state), Value::string("yes"), Value::string("no"));
        let values = if function == "dict" {
            (0..count)
                .flat_map(|index| [Value::string(&index.to_string()), choice.clone()])
                .collect()
        } else {
            vec![choice; count]
        };
        let Measured { exits: _, work } = measure(|| invoke(&executor, function, values, state));
        measured.push(work);
    }
    sim_assert_eq!(have: measured, want: (0..5).map(|_| Work {
        alternatives: 1, allocations: usize::from(function == "dict"), exits: 1, ..Work::default()
    }).collect::<Vec<_>>());
}

#[test]
fn correlated_list_members_do_not_fork_execution() {
    correlated_constructor("list");
}

#[test]
fn correlated_tuple_members_do_not_fork_execution() {
    correlated_constructor("tuple");
}

#[test]
fn correlated_dictionary_members_do_not_fork_execution() {
    correlated_constructor("dict");
}

#[test]
fn correlated_constructor_chain_enters_include_once() -> eyre::Result<()> {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "templates/_helpers.tpl",
        r#"{{ define "echo" }}{{ index .items 0 }}{{ end }}"#,
    );
    let db = IrAnalysisDb::new(&defines);
    let executor = Executor { db: &db };
    let mut state = State::new(Value::nil());
    state.namespace = crate::value::namespace::Namespace::from_sources(
        db.template_sources().to_vec(),
        crate::analysis_db::DefinitionInventory::Complete,
        &state.heap.arena,
    )
    .namespace;
    let test = decision(&state);
    let choice = Value::select(Rc::clone(&test), Value::string("yes"), Value::string("no"));
    let Measured { exits, work } = measure(|| {
        let mut results = Vec::new();
        for listed in invoke(&executor, "list", vec![choice.clone(), choice], state) {
            let Flow::Normal(list) = listed.flow else {
                continue;
            };
            for mapped in invoke(
                &executor,
                "dict",
                vec![Value::string("items"), list.value],
                listed.state,
            ) {
                let Flow::Normal(map) = mapped.flow else {
                    continue;
                };
                results.extend(invoke(
                    &executor,
                    "include",
                    vec![Value::string("echo"), map.value],
                    mapped.state,
                ));
            }
        }
        results
    });
    sim_assert_eq!(have: work, want: Work { alternatives: 3, forks: 0, allocations: 1, helpers: 1, exits: 1 });
    let exit = exits.first().ok_or_eyre("include exit missing")?;
    let Flow::Normal(normal) = &exit.flow else {
        eyre::bail!("include aborted")
    };
    for (taken, expected) in [(true, "yes"), (false, "no")] {
        let state = exit.state.fork(Rc::clone(&test), taken);
        let value = state.consumed(&normal.value);
        sim_assert_eq!(have: state.heap.freeze(&value).exact_text(), want: Some(expected.to_owned()));
    }
    Ok(())
}

#[test]
fn conditional_dictionary_keys_still_split_and_repeated_keys_keep_the_last_member()
-> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    let state = State::new(Value::nil());
    let test = decision(&state);
    let key = Value::select(Rc::clone(&test), Value::string("x"), Value::string("y"));
    let value = Value::select(Rc::clone(&test), Value::string("yes"), Value::string("no"));
    let Measured { exits, work } = measure(|| {
        invoke(
            &executor,
            "dict",
            vec![Value::string("x"), Value::string("old"), key, value],
            state,
        )
    });
    sim_assert_eq!(have: work, want: Work { alternatives: 2, forks: 2, allocations: 2, helpers: 0, exits: 1 });
    for exit in exits {
        let Flow::Normal(normal) = exit.flow else {
            eyre::bail!("dict aborted")
        };
        for taken in [true, false] {
            let mut state = exit.state.fork(Rc::clone(&test), taken);
            let x = state.get_field(&normal.value, "x");
            let y = state.get_field(&normal.value, "y");
            sim_assert_eq!(have: x.constant(), want: Some(&GuardValue::String(if taken { "yes" } else { "old" }.to_owned())));
            sim_assert_eq!(have: y.constant(), want: Some(&GuardValue::String(if taken { "" } else { "no" }.to_owned())));
        }
    }
    Ok(())
}

#[test]
fn constructor_payload_choices_remain_exact_when_serialized() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    for function in ["list", "dict"] {
        let state = State::new(Value::nil());
        let test = decision(&state);
        let choice = Value::select(Rc::clone(&test), Value::string("yes"), Value::string("no"));
        let values = if function == "dict" {
            vec![Value::string("x"), choice]
        } else {
            vec![choice]
        };
        for constructed in invoke(&executor, function, values, state) {
            let Flow::Normal(normal) = constructed.flow else {
                eyre::bail!("constructor aborted")
            };
            for serialized in invoke(&executor, "toJson", vec![normal.value], constructed.state) {
                let Flow::Normal(normal) = serialized.flow else {
                    eyre::bail!("serialization aborted")
                };
                for taken in [true, false] {
                    let branch = serialized.state.fork(Rc::clone(&test), taken);
                    if branch.path_is_infeasible() {
                        continue;
                    }
                    let spelling = if taken { "yes" } else { "no" };
                    let expected = if function == "dict" {
                        format!(r#"{{"x":"{spelling}"}}"#)
                    } else {
                        format!(r#"["{spelling}"]"#)
                    };
                    let value = branch.consumed(&normal.value);
                    sim_assert_eq!(have: branch.heap.freeze(&value).exact_text(), want: Some(expected));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn independent_constructor_members_do_not_form_an_execution_product() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    let mut measured = std::collections::BTreeMap::new();
    let mut expected = std::collections::BTreeMap::new();
    for function in ["list", "dict"] {
        for count in [2, 4, 6] {
            let state = State::new(Value::nil());
            let values = (0..count)
                .flat_map(|index| {
                    let predicate = Predicate::truthy_path(format!("choice{index}"));
                    let test = state.heap.arena.decision(
                        predicate.clone(),
                        predicate.negated(),
                        Coverage::Exact,
                    );
                    let choice = Value::select(test, Value::string("yes"), Value::string("no"));
                    if function == "dict" {
                        vec![Value::string(&index.to_string()), choice]
                    } else {
                        vec![choice]
                    }
                })
                .collect();
            let Measured { exits: _, work } =
                measure(|| invoke(&executor, function, values, state));
            let key = format!("{function} members={count}");
            measured.insert(key.clone(), work);
            expected.insert(
                key,
                Work {
                    alternatives: 1,
                    allocations: usize::from(function == "dict"),
                    exits: 1,
                    ..Work::default()
                },
            );
        }
    }
    sim_assert_eq!(have: measured, want: expected);
}

#[test]
fn inspected_members_preserve_partial_choice_correlations_without_state_forks() -> eyre::Result<()>
{
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    for function in ["toJson", "join"] {
        let state = State::new(Value::nil());
        // Neither arm has an input predicate; only the shared runtime choice relates them.
        let test = state
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial);
        let members = Value::sequence(vec![
            Value::select(Rc::clone(&test), Value::string("yes"), Value::string("no")),
            Value::select(Rc::clone(&test), Value::string("no"), Value::string("yes")),
        ]);
        let arguments = if function == "join" {
            vec![Value::string("|"), members]
        } else {
            vec![members]
        };
        let Measured { exits, work } = measure(|| invoke(&executor, function, arguments, state));
        sim_assert_eq!(have: work.forks, want: 0);
        sim_assert_eq!(have: exits.len(), want: 1);
        let exit = exits.first().ok_or_eyre("consumer exit missing")?;
        let Flow::Normal(normal) = &exit.flow else {
            eyre::bail!("consumer aborted");
        };
        for taken in [true, false] {
            let branch = exit.state.fork(Rc::clone(&test), taken);
            let expected = match (function, taken) {
                ("join", true) => "yes|no",
                ("join", false) => "no|yes",
                (_, true) => r#"["yes","no"]"#,
                (_, false) => r#"["no","yes"]"#,
            };
            let value = branch.consumed(&normal.value);
            sim_assert_eq!(have: branch.heap.freeze(&value).exact_text(), want: Some(expected.to_owned()));
        }
    }
    Ok(())
}

#[test]
fn constructor_argument_order_and_aliases_keep_frozen_serialization() {
    let source = indoc! {r#"
        {{- $map := dict "x" 0 -}}
        {{- $items := list (set $map "x" 1) (set $map "x" 2) -}}
        {{- $before := toJson $items -}}
        {{- $_ := set $map "x" 3 -}}
        {{- $before -}}|{{- toJson $items -}}
    "#};
    sim_assert_eq!(
        have: super::texts(source),
        want: vec![Some(r#"[{"x":2},{"x":2}]|[{"x":3},{"x":3}]"#.to_owned())]
    );
}

#[test]
fn guarded_null_members_are_dropped_by_string_consumers() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    for function in ["join", "toStrings", "sortAlpha"] {
        let state = State::new(Value::nil());
        let test = decision(&state);
        let items = Value::sequence(vec![
            Value::string("z"),
            Value::select(Rc::clone(&test), Value::nil(), Value::string("a")),
        ]);
        let arguments = if function == "join" {
            vec![Value::string("|"), items]
        } else {
            vec![items]
        };
        for result in invoke(&executor, function, arguments, state) {
            let Flow::Normal(normal) = result.flow else {
                eyre::bail!("string consumer aborted");
            };
            let exits = if function == "join" {
                vec![Exit {
                    state: result.state,
                    flow: Flow::Normal(normal),
                }]
            } else {
                invoke(
                    &executor,
                    "join",
                    vec![Value::string("|"), normal.value],
                    result.state,
                )
            };
            for exit in exits {
                let Flow::Normal(normal) = exit.flow else {
                    eyre::bail!("join aborted");
                };
                for taken in [true, false] {
                    let branch = exit.state.fork(Rc::clone(&test), taken);
                    if branch.path_is_infeasible() {
                        continue;
                    }
                    let expected = if taken {
                        "z"
                    } else if function == "sortAlpha" {
                        "a|z"
                    } else {
                        "z|a"
                    };
                    let value = branch.consumed(&normal.value);
                    sim_assert_eq!(have: branch.heap.freeze(&value).exact_text(), want: Some(expected.to_owned()));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn guarded_field_presence_serializes_from_the_frozen_snapshot() -> eyre::Result<()> {
    use crate::value::{Field, Remainder};

    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    for function in ["toJson", "toYaml"] {
        let mut state = State::new(Value::nil());
        let test = decision(&state);
        let value = state.heap.map(Remainder::Closed);
        let id = state
            .map_target(&value)
            .ok_or_eyre("map identity missing")?;
        state.heap.write(
            &id,
            "key",
            Field::select(
                Rc::clone(&test),
                Field::Present(Value::string("old")),
                Field::Absent,
            ),
        );
        for mut exit in invoke(&executor, function, vec![value], state) {
            let Flow::Normal(normal) = exit.flow else {
                eyre::bail!("serialization aborted");
            };
            exit.state
                .heap
                .write(&id, "key", Field::Present(Value::string("new")));
            for taken in [true, false] {
                let branch = exit.state.fork(Rc::clone(&test), taken);
                if branch.path_is_infeasible() {
                    continue;
                }
                let expected = match (function, taken) {
                    (_, false) => "{}",
                    ("toJson", true) => r#"{"key":"old"}"#,
                    (_, true) => "key: old",
                };
                let value = branch.consumed(&normal.value);
                sim_assert_eq!(have: branch.heap.freeze(&value).exact_text(), want: Some(expected.to_owned()));
            }
        }
    }
    Ok(())
}

#[test]
fn guarded_constructor_members_keep_the_128_member_bound() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    for count in [127, 128, 129] {
        let state = State::new(Value::nil());
        let test = decision(&state);
        let item = Value::select(Rc::clone(&test), Value::string("yes"), Value::string("no"));
        let Measured { exits, work } =
            measure(|| invoke(&executor, "list", vec![item; count], state));
        sim_assert_eq!(have: work.forks, want: 0);
        sim_assert_eq!(have: work.alternatives, want: 1);
        for exit in exits {
            let Flow::Normal(normal) = exit.flow else {
                eyre::bail!("list aborted");
            };
            for serialized in invoke(&executor, "toJson", vec![normal.value], exit.state) {
                let Flow::Normal(normal) = serialized.flow else {
                    eyre::bail!("JSON aborted");
                };
                for taken in [true, false] {
                    let branch = serialized.state.fork(Rc::clone(&test), taken);
                    if branch.path_is_infeasible() {
                        continue;
                    }
                    let value = branch.consumed(&normal.value);
                    let rendered = branch.heap.freeze(&value).exact_text();
                    let expected = if count <= 128 {
                        Some(serde_json::to_string(&vec![
                            if taken {
                                "yes"
                            } else {
                                "no"
                            };
                            count
                        ])?)
                    } else {
                        None
                    };
                    sim_assert_eq!(have: rendered, want: expected);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn guarded_merge_members_are_inspected_before_mutation() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    let state = State::new(Value::nil());
    let test = decision(&state);
    let member = Value::select(
        Rc::clone(&test),
        Value::string(""),
        Value::string("destination"),
    );
    for destination in invoke(&executor, "dict", vec![Value::string("key"), member], state) {
        let Flow::Normal(destination_value) = destination.flow else {
            eyre::bail!("destination aborted");
        };
        for source in invoke(
            &executor,
            "dict",
            vec![Value::string("key"), Value::string("source")],
            destination.state,
        ) {
            let Flow::Normal(source_value) = source.flow else {
                eyre::bail!("source aborted");
            };
            for merged in invoke(
                &executor,
                "merge",
                vec![destination_value.value.clone(), source_value.value],
                source.state,
            ) {
                let Flow::Normal(normal) = merged.flow else {
                    eyre::bail!("merge aborted");
                };
                for taken in [true, false] {
                    let mut branch = merged.state.fork(Rc::clone(&test), taken);
                    if branch.path_is_infeasible() {
                        continue;
                    }
                    let value = branch.get_field(&normal.value, "key");
                    sim_assert_eq!(have: value.constant(), want: Some(&GuardValue::String(if taken { "source" } else { "destination" }.to_owned())));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn guarded_members_preserve_the_concrete_json_depth_boundary() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let executor = Executor { db: &db };
    for depth in [0, 30, 31, 32, 33] {
        let state = State::new(Value::nil());
        let test = decision(&state);
        let mut member = Value::select(Rc::clone(&test), Value::string("yes"), Value::string("no"));
        for _ in 0..depth {
            member = Value::sequence(vec![member]);
        }
        for serialized in invoke(&executor, "toJson", vec![member], state) {
            let Flow::Normal(normal) = serialized.flow else {
                eyre::bail!("JSON aborted");
            };
            for taken in [true, false] {
                let branch = serialized.state.fork(Rc::clone(&test), taken);
                let value = branch.consumed(&normal.value);
                let mut expected =
                    serde_json::Value::String(if taken { "yes" } else { "no" }.to_owned());
                for _ in 0..depth {
                    expected = serde_json::Value::Array(vec![expected]);
                }
                let expected = if depth <= 32 {
                    Some(serde_json::to_string(&expected)?)
                } else {
                    None
                };
                sim_assert_eq!(have: branch.heap.freeze(&value).exact_text(), want: expected);
            }
        }
    }
    Ok(())
}
