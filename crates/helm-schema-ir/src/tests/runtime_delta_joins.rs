use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use helm_schema_syntax::Span;
use test_util::prelude::sim_assert_eq;

use crate::value::state::ContinuationJoin;
use crate::value::{AbortReason, Coverage, Exit, Flow, FrozenTest, LoopId, Normal, State, Value};

fn decision(state: &State) -> Rc<FrozenTest> {
    let condition = Predicate::from(Guard::Truthy {
        path: ValuesPath::parse("branch"),
    });
    state
        .heap
        .arena
        .decision(condition.clone(), condition.negated(), Coverage::Exact)
}

fn join(yes: &State, no: &State, decision: &Rc<FrozenTest>) -> eyre::Result<State> {
    State::join_branches(
        &ContinuationJoin {
            decision: Rc::clone(decision),
            active: Vec::new(),
            taken: true,
        },
        yes,
        no,
    )
    .ok_or_eyre("compatible branch deltas must join")
}

#[test]
fn unchanged_branch_join_reuses_every_frame_and_history_prefix() -> eyre::Result<()> {
    let mut parent = State::new(Value::nil());
    for depth in 0..8 {
        parent.enter_scope(None);
        parent.bind(format!("v{depth}"), Value::literal(GuardValue::Int(depth)));
    }
    for ordinal in 0..64 {
        parent.emit_bytes(Span::new(ordinal, ordinal + 1), None, "prefix");
    }
    let test = decision(&parent);
    let yes = parent.fork(Rc::clone(&test), true);
    let no = parent.fork(Rc::clone(&test), false);
    let joined = join(&yes, &no, &test)?;
    // The shared parent contributes no copied frame or observation payload.
    assert!(
        joined
            .frames
            .iter()
            .zip(&parent.frames)
            .all(|(joined, parent)| Rc::ptr_eq(joined, parent))
    );
    assert!(
        joined
            .text_sites
            .iter()
            .zip(parent.text_sites.iter())
            .all(|(joined, parent)| std::ptr::eq(joined, parent))
    );
    sim_assert_eq!(have: joined.text_sites.len(), want: 64);
    Ok(())
}

#[test]
fn changed_binding_joins_under_its_original_decision_and_preserves_other_frames() -> eyre::Result<()>
{
    let mut parent = State::new(Value::nil());
    parent.bind("changed".to_owned(), Value::string("parent"));
    parent.enter_scope(None);
    parent.bind("untouched".to_owned(), Value::string("shared"));
    let test = decision(&parent);
    let mut yes = parent.fork(Rc::clone(&test), true);
    let mut no = parent.fork(Rc::clone(&test), false);
    assert!(yes.assign("changed", Value::string("yes")));
    assert!(no.assign("changed", Value::string("no")));
    let joined = join(&yes, &no, &test)?;
    let inner = joined.frames.last().ok_or_eyre("joined inner frame")?;
    assert!(Rc::ptr_eq(
        inner,
        parent.frames.last().ok_or_eyre("parent inner frame")?
    ));
    let changed = joined.read_local("changed").ok_or_eyre("joined binding")?;
    for (taken, expected) in [(true, "yes"), (false, "no")] {
        sim_assert_eq!(have: changed.under(&test, taken).constant().cloned(), want: Some(GuardValue::String(expected.to_owned())));
    }
    Ok(())
}

#[test]
fn observation_deltas_follow_the_inherited_prefix_in_branch_order() -> eyre::Result<()> {
    let mut parent = State::new(Value::nil());
    parent.emit_bytes(Span::new(0, 1), None, "prefix");
    let test = decision(&parent);
    let mut yes = parent.fork(Rc::clone(&test), true);
    let mut no = parent.fork(Rc::clone(&test), false);
    yes.emit_bytes(Span::new(2, 3), None, "yes");
    no.emit_bytes(Span::new(4, 5), None, "no");
    let joined = join(&yes, &no, &test)?;
    sim_assert_eq!(have: joined.text_sites.iter().map(|site| site.bytes.as_str()).collect::<Vec<_>>(), want: vec!["prefix", "yes", "no"]);
    for (taken, expected) in [(true, "prefixyes"), (false, "prefixno")] {
        sim_assert_eq!(have: joined.output.as_value().under(&test, taken).constant().cloned(), want: Some(GuardValue::String(expected.to_owned())));
    }
    Ok(())
}

#[test]
fn continuation_keys_follow_names_but_not_declaration_order_or_values() {
    let parent = State::new(Value::nil());
    let test = decision(&parent);
    let mut yes = parent.fork(Rc::clone(&test), true);
    let mut no = parent.fork(Rc::clone(&test), false);
    yes.bind("a".to_owned(), Value::string("a"));
    yes.bind("b".to_owned(), Value::string("b"));
    no.bind("b".to_owned(), Value::string("other b"));
    no.bind("a".to_owned(), Value::string("other a"));
    assert!(yes.continuation_key() == no.continuation_key());
    no.bind("another".to_owned(), Value::nil());
    assert!(yes.continuation_key() != no.continuation_key());
    no.enter_scope(None);
    no.bind("local".to_owned(), Value::nil());
    assert!(no.leave_scope());
    assert!(yes.continuation_key() != no.continuation_key());
}

#[test]
fn keyed_normal_join_keeps_terminal_flows_and_their_order() -> eyre::Result<()> {
    let parent = State::new(Value::nil());
    let test = decision(&parent);
    let loop_id = LoopId {
        call: 42,
        ordinal: 7,
    };
    let exits = crate::value::control::reconverge(vec![
        Exit {
            state: parent.fork(Rc::clone(&test), true),
            flow: Flow::Normal(Normal {
                value: Value::string("yes"),
            }),
        },
        Exit {
            state: parent.clone(),
            flow: Flow::Abort(AbortReason::Fail),
        },
        Exit {
            state: parent.fork(Rc::clone(&test), false),
            flow: Flow::Normal(Normal {
                value: Value::string("no"),
            }),
        },
        Exit {
            state: parent.clone(),
            flow: Flow::Break(loop_id),
        },
        Exit {
            state: parent,
            flow: Flow::Continue(loop_id),
        },
    ]);
    let labels = exits
        .iter()
        .map(|exit| match exit.flow {
            Flow::Normal(_) => "normal",
            Flow::Abort(_) => "abort",
            Flow::Break(_) => "break",
            Flow::Continue(_) => "continue",
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(have: labels, want: vec!["abort", "normal", "break", "continue"]);
    let value = exits
        .iter()
        .find_map(|exit| match &exit.flow {
            Flow::Normal(value) => Some(&value.value),
            Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => None,
        })
        .ok_or_eyre("joined normal")?;
    for (taken, expected) in [(true, "yes"), (false, "no")] {
        sim_assert_eq!(have: value.under(&test, taken).constant().cloned(), want: Some(GuardValue::String(expected.to_owned())));
    }
    Ok(())
}
