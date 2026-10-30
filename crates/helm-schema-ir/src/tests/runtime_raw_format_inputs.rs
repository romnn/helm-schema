use std::collections::BTreeSet;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::{PayloadQuery, RawFormatInput};
use crate::value::{
    AliasSet, Coverage, Datum, Field, Leaf, MapType, ObjectId, Origin, State, TypeCoverage,
    UnknownValue, Value, ValueNode,
};

fn raw_format_inputs(value: &Value, state: &State) -> Vec<RawFormatInput> {
    PayloadQuery::new(&state.heap.arena.input_objects.borrow())
        .raw_format_inputs(&state.heap.freeze(value))
}

fn input_state() -> eyre::Result<State> {
    let mut state = State::new(Value::nil());
    state
        .map_target(&Value::input(ValuesPath::default()))
        .ok_or_eyre("input root backing missing")?;
    Ok(state)
}

fn materialize(state: &mut State, path: &str) -> eyre::Result<ObjectId> {
    let value = Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Unknown(UnknownValue {
                types: TypeCoverage::Map(MapType::Interface),
                aliases: AliasSet::RenderGraph,
            }),
            origin: Origin {
                identity: Some(ValuesPath::parse(path)),
                influences: BTreeSet::new(),
            },
        })),
    };
    state
        .map_target(&value)
        .ok_or_eyre("typed input backing missing")
}

#[test]
fn raw_format_capture_keeps_exact_selected_paths_and_snapshot_proofs() -> eyre::Result<()> {
    let mut state = input_state()?;
    let condition = Predicate::truthy_path("choose");
    let decision = state.heap.arena.decision(
        condition.clone(),
        condition.clone().negated(),
        Coverage::Exact,
    );
    let value = Value::select(
        decision,
        Value::input(ValuesPath::parse("x")),
        Value::input(ValuesPath::parse("y")),
    );
    state.observe(helm_schema_syntax::Span::new(0, 1), None, &value, true);
    let captured = state
        .sites
        .iter()
        .next()
        .ok_or_eyre("observation missing")?;
    sim_assert_eq!(have: raw_format_inputs(&captured.value.value, &state), want: vec![RawFormatInput { condition: condition.clone(), path: ValuesPath::parse("x") }, RawFormatInput { condition: condition.negated(), path: ValuesPath::parse("y") }]);
    let before = raw_format_inputs(&captured.value.value, &state);
    let root = state
        .map_target(&Value::input(ValuesPath::default()))
        .ok_or_eyre("root backing missing")?;
    state
        .heap
        .write(&root, "x", Field::Present(Value::string("replacement")));
    sim_assert_eq!(have: { let captured = state.sites.iter().next().ok_or_eyre("frozen observation missing")?; PayloadQuery::new(&state.heap.arena.input_objects.borrow()).raw_format_inputs(&captured.value) }, want: before);
    sim_assert_eq!(have: raw_format_inputs(&Value::input(ValuesPath::parse("x")), &state), want: Vec::<RawFormatInput>::new());
    Ok(())
}

#[test]
fn materialized_input_maps_recover_only_their_original_pristine_backing() -> eyre::Result<()> {
    let mut state = input_state()?;
    let object = materialize(&mut state, "x")?;
    let map = Value::leaf(Datum::Map {
        view: MapType::Interface,
        target: Some(object.clone()),
    });
    sim_assert_eq!(have: raw_format_inputs(&map, &state), want: vec![RawFormatInput { condition: Predicate::True, path: ValuesPath::parse("x") }]);
    let root = state
        .map_target(&Value::input(ValuesPath::default()))
        .ok_or_eyre("root missing")?;
    state
        .heap
        .write(&root, "unrelated", Field::Present(Value::string("sibling")));
    sim_assert_eq!(have: raw_format_inputs(&map, &state).len(), want: 1);
    let _neighbor = materialize(&mut state, "x!")?;
    let descendant = materialize(&mut state, "x.child")?;
    state.heap.write(
        &descendant,
        "changed",
        Field::Present(Value::string("mutation")),
    );
    sim_assert_eq!(have: raw_format_inputs(&map, &state), want: Vec::<RawFormatInput>::new());
    sim_assert_eq!(have: raw_format_inputs(&Value::input(ValuesPath::parse("z")), &state), want: vec![RawFormatInput { condition: Predicate::True, path: ValuesPath::parse("z") }]);
    Ok(())
}

#[test]
fn conditional_and_unknown_writers_never_keep_original_input_proofs() -> eyre::Result<()> {
    let mut initial = input_state()?;
    let object = materialize(&mut initial, "x")?;
    let condition = Predicate::truthy_path("choose");
    let decision =
        initial
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let mut yes = initial.fork(Rc::clone(&decision), true);
    let no = initial.fork(Rc::clone(&decision), false);
    yes.heap.write(
        &object,
        "changed",
        Field::Present(Value::string("mutation")),
    );
    let joined = State::join_branches(
        &crate::value::state::ContinuationJoin {
            decision,
            active: Vec::new(),
            taken: true,
        },
        &yes,
        &no,
    )
    .ok_or_eyre("conditional input join missing")?;
    sim_assert_eq!(have: raw_format_inputs(&Value::input(ValuesPath::parse("x")), &joined), want: Vec::<RawFormatInput>::new());
    crate::value::operations::invalidate(&mut initial);
    sim_assert_eq!(have: raw_format_inputs(&Value::input(ValuesPath::parse("fresh")), &initial), want: Vec::<RawFormatInput>::new());
    Ok(())
}

#[test]
fn derived_or_partial_values_do_not_claim_original_format_inputs() -> eyre::Result<()> {
    let state = input_state()?;
    let rendered = Value::scalar(
        crate::value::ScalarType::String,
        Some(crate::value::scalar_value::ScalarContent::Rendered(vec![
            crate::value::scalar_value::ScalarRenderPart::Identity {
                path: ValuesPath::parse("x"),
                stringified: true,
                lexical_escapes: BTreeSet::new(),
            },
        ])),
    );
    sim_assert_eq!(have: raw_format_inputs(&rendered, &state), want: Vec::<RawFormatInput>::new());
    let decision = state
        .heap
        .arena
        .decision(Predicate::False, Predicate::False, Coverage::Partial);
    let partial = Value::select(
        decision,
        Value::input(ValuesPath::parse("x")),
        Value::input(ValuesPath::parse("y")),
    );
    sim_assert_eq!(have: raw_format_inputs(&partial, &state), want: Vec::<RawFormatInput>::new());
    let mut too_many = Value::input(ValuesPath::parse("tail"));
    for index in 0..128 {
        let condition = Predicate::truthy_path(format!("choose{index}"));
        let decision =
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact);
        too_many = Value::select(
            decision,
            Value::input(ValuesPath::parse(&format!("x{index}"))),
            too_many,
        );
    }
    sim_assert_eq!(have: raw_format_inputs(&too_many, &state), want: Vec::<RawFormatInput>::new());
    Ok(())
}

#[test]
fn empty_key_mutations_are_not_hidden_by_encoded_sibling_paths() -> eyre::Result<()> {
    let mut state = input_state()?;
    let _source = materialize(&mut state, "x")?;
    let interleaved_sibling = materialize(&mut state, "x!")?;
    let escaped_dot_sibling = materialize(&mut state, r"x\.child")?;
    for sibling in [interleaved_sibling, escaped_dot_sibling] {
        state.heap.write(
            &sibling,
            "changed",
            Field::Present(Value::string("sibling mutation")),
        );
    }
    let descendant = materialize(&mut state, "x.child")?;
    let source = Value::input(ValuesPath::parse("x"));
    sim_assert_eq!(have: raw_format_inputs(&source, &state), want: vec![RawFormatInput { condition: Predicate::True, path: ValuesPath::parse("x") }]);
    // Empty Go map keys live in backing fields; ValuesPath deliberately drops empty segments.
    state.heap.write(
        &descendant,
        "",
        Field::Present(Value::string("empty-key mutation")),
    );
    sim_assert_eq!(have: raw_format_inputs(&source, &state), want: Vec::<RawFormatInput>::new());
    Ok(())
}
