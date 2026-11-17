use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::InputCache;
use crate::value::work::WorkBudget;
use crate::value::{
    Coverage, Datum, Field, FrozenValue, MapType, Multiplicity, ObjectData, Reach, Remainder,
    State, TypeCoverage, UnknownValue, Value,
};

#[derive(Debug, PartialEq, Eq)]
struct Observation {
    inputs: BTreeSet<ValuesPath>,
    remaining: usize,
    exhausted: bool,
}

fn observe(value: &FrozenValue, aliases: bool, remaining: usize) -> Observation {
    let mut budget = WorkBudget::new(remaining);
    let inputs = value.collect_inputs_with_budget(aliases, &mut budget);
    Observation {
        inputs,
        remaining: budget.remaining,
        exhausted: budget.is_exhausted(),
    }
}

#[test]
fn cached_empty_map_exhausts_at_the_cold_traversal_boundary() {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let frozen = state.heap.freeze(&map);
    let expected = Observation {
        inputs: BTreeSet::from([ValuesPath::default()]),
        remaining: 0,
        exhausted: true,
    };
    sim_assert_eq!(have: observe(&frozen, true, 2), want: expected);
    // A completed traversal costs two visits even though its result has no inputs.
    sim_assert_eq!(have: observe(&frozen, true, 3), want: Observation {
        inputs: BTreeSet::new(), remaining: 1, exhausted: false,
    });
    sim_assert_eq!(have: observe(&frozen, true, 2), want: expected);
}

#[test]
fn cached_provenance_cannot_change_persistent_widened_contents() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let frozen = state.heap.freeze(&map);
    let mut cold = state.heap.fork();
    let mut warm = state.heap.fork();
    // Scalar visits leave two units before the final map is consumed and queried.
    let mut slots = vec![Value::nil(); 32_767];
    slots.push(map);
    let cold_id = cold.allocate(ObjectData::Strings(slots.clone()), Multiplicity::Singleton);
    assert!(!observe(&frozen, false, 65_536).exhausted);
    let warm_id = warm.allocate(ObjectData::Strings(slots), Multiplicity::Singleton);
    let expected = ObjectData::Widened {
        inputs: Rc::new(BTreeSet::from([ValuesPath::default()])),
    };
    sim_assert_eq!(have: &cold.version.object(&cold_id).ok_or_eyre("cold object missing")?.data, want: &expected);
    sim_assert_eq!(have: &warm.version.object(&warm_id).ok_or_eyre("warm object missing")?.data, want: &expected);
    Ok(())
}

fn next(random: &mut usize, bound: usize) -> usize {
    *random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*random >> 16) % bound
}

fn randomized_roots(state: &mut State, random: &mut usize) -> eyre::Result<Vec<FrozenValue>> {
    let empty = state.heap.map(Remainder::Closed);
    let mut values = vec![empty.clone()];
    let mut objects = vec![state.map_target(&empty).ok_or_eyre("empty map missing")?];
    let mut roots = vec![state.heap.freeze(&empty)];
    // Shared values, aliases, branches and snapshot versions vary independently of cache state.
    for ordinal in 0..32 {
        let inputs = BTreeSet::from([ValuesPath::parse(&format!("source{ordinal}"))]);
        let remainder = match next(random, 4) {
            0 => Remainder::Closed,
            1 => Remainder::Input,
            2 => Remainder::Open,
            _ => Remainder::Unknown {
                inputs: Rc::new(inputs.clone()),
            },
        };
        let map = state.heap.map(remainder);
        let target = state.map_target(&map).ok_or_eyre("generated map missing")?;
        for field in 0..next(random, 8) {
            let value = match next(random, 6) {
                0 => Value::with_influences(Value::string("text"), inputs.clone()),
                1 => values
                    .get(next(random, values.len()))
                    .ok_or_eyre("shared value missing")?
                    .clone(),
                2 => Value::leaf(Datum::Unknown(UnknownValue {
                    types: TypeCoverage::Unresolved,
                    aliases: Reach::Bounded(Rc::new(BTreeSet::from([objects
                        .get(next(random, objects.len()))
                        .ok_or_eyre("alias missing")?
                        .clone()]))),
                })),
                3 => Value::leaf(Datum::Unknown(UnknownValue {
                    types: TypeCoverage::Unresolved,
                    aliases: Reach::Top,
                })),
                4 => Value::string("constant"),
                _ => {
                    let condition = Predicate::truthy_path(format!("branch{ordinal}"));
                    let test = state.heap.arena.decision(
                        condition.clone(),
                        condition.negated(),
                        Coverage::Exact,
                    );
                    Value::select(
                        test,
                        values
                            .get(next(random, values.len()))
                            .ok_or_eyre("branch missing")?
                            .clone(),
                        Value::with_influences(Value::string("other"), inputs.clone()),
                    )
                }
            };
            state
                .heap
                .write(&target, &format!("field{field}"), Field::Present(value));
        }
        values.push(map.clone());
        objects.push(target);
        roots.push(state.heap.freeze(&map));
        if ordinal % 4 == 0 {
            let length = next(random, 5);
            let backing = state.heap.allocate(
                ObjectData::Strings(vec![
                    Value::with_influences(Value::string("slot"), inputs);
                    length
                ]),
                Multiplicity::Singleton,
            );
            let sequence = Value::strings(backing, length);
            roots.push(state.heap.freeze(&sequence));
        }
    }
    Ok(roots)
}

#[test]
fn randomized_provenance_answers_and_charges_ignore_cache_state() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let mut random = 0x4a2;
    let roots = randomized_roots(&mut state, &mut random)?;
    let mut tiny_hits = 0;
    let mut tiny_misses = 0;
    for (index, root) in roots.iter().enumerate() {
        for aliases in [false, true] {
            *state.heap.arena.input_queries.borrow_mut() = InputCache::default();
            let complete = observe(root, aliases, 65_536);
            assert!(!complete.exhausted);
            let cost = 65_536 - complete.remaining;
            for remaining in [
                0,
                1,
                2,
                cost.saturating_sub(1),
                cost,
                cost + 1,
                cost + 2,
                next(&mut random, cost + 3),
            ] {
                *state.heap.arena.input_queries.borrow_mut() = InputCache::default();
                let cold = observe(root, aliases, remaining);
                *state.heap.arena.input_queries.borrow_mut() = InputCache::default();
                assert!(!observe(root, aliases, 65_536).exhausted);
                sim_assert_eq!(have: observe(root, aliases, remaining), want: cold);

                *state.heap.arena.input_queries.borrow_mut() = InputCache {
                    max_entries: 1,
                    ..InputCache::default()
                };
                assert!(!observe(root, aliases, 65_536).exhausted);
                if index % 2 != 0 {
                    let decoy = roots.first().ok_or_eyre("decoy missing")?;
                    assert!(!observe(decoy, aliases, 65_536).exhausted);
                }
                let hits = state.heap.arena.input_queries.borrow().stats.hits;
                sim_assert_eq!(have: observe(root, aliases, remaining), want: cold);
                let cache = state.heap.arena.input_queries.borrow();
                assert!(cache.entries.len() <= 1);
                if cache.stats.hits > hits {
                    tiny_hits += 1;
                } else {
                    tiny_misses += 1;
                }
            }
        }
    }
    assert!(
        tiny_hits > 0 && tiny_misses > 0,
        "the one-entry cache must exercise hits and eviction misses"
    );
    Ok(())
}

fn union_results(shared: bool) -> (Rc<BTreeSet<ValuesPath>>, Rc<BTreeSet<ValuesPath>>) {
    use crate::value::heap::{InputUnions, join_input_sets};
    let yes = Rc::new(BTreeSet::from([ValuesPath::parse("x")]));
    let no = Rc::new(BTreeSet::from([ValuesPath::parse("y")]));
    let memo = RefCell::new(InputUnions::default());
    let first = join_input_sets(&yes, &no, Some(&memo), &mut WorkBudget::new(64));
    if !shared {
        *memo.borrow_mut() = InputUnions::default();
    }
    let second = join_input_sets(&yes, &no, Some(&memo), &mut WorkBudget::new(64));
    sim_assert_eq!(have: Rc::ptr_eq(&first, &second), want: shared);
    (first, second)
}

#[test]
fn union_memo_sharing_cannot_discount_later_union_charges() {
    use crate::value::heap::join_input_sets;
    let (shared_yes, shared_no) = union_results(true);
    let (separate_yes, separate_no) = union_results(false);
    for remaining in 0..8 {
        let mut warm = WorkBudget::new(remaining);
        let warm_inputs = join_input_sets(&shared_yes, &shared_no, None, &mut warm);
        let mut cold = WorkBudget::new(remaining);
        let cold_inputs = join_input_sets(&separate_yes, &separate_no, None, &mut cold);
        sim_assert_eq!(have: (warm_inputs, warm.remaining, warm.is_exhausted()), want: (cold_inputs, cold.remaining, cold.is_exhausted()));
    }
}

fn unknown_data(
    state: &State,
    inputs: Rc<BTreeSet<ValuesPath>>,
    shape: usize,
) -> eyre::Result<ObjectData> {
    Ok(match shape {
        0 => ObjectData::Widened { inputs },
        1 => ObjectData::Map {
            fields: Rc::default(),
            remainder: Remainder::Unknown { inputs },
        },
        _ => {
            let condition = Predicate::truthy_path("layer");
            let decision =
                state
                    .heap
                    .arena
                    .decision(condition.clone(), condition.negated(), Coverage::Exact);
            let remainder = Remainder::Unknown { inputs };
            ObjectData::Map {
                fields: Rc::default(),
                remainder: crate::value::layers::join_remainders(
                    &decision,
                    &remainder,
                    &remainder,
                    &mut WorkBudget::new(64),
                )
                .ok_or_eyre("layered remainder missing")?,
            }
        }
    })
}

fn union_root(shared: bool, shapes: [usize; 2]) -> eyre::Result<FrozenValue> {
    let mut state = State::new(Value::nil());
    let root = state.heap.map(Remainder::Closed);
    let root_id = state.map_target(&root).ok_or_eyre("root missing")?;
    let (first, second) = union_results(shared);
    for (index, inputs) in [first, second].into_iter().enumerate() {
        let data = unknown_data(
            &state,
            inputs,
            *shapes.get(index).ok_or_eyre("shape missing")?,
        )?;
        let object = state.heap.allocate(data, Multiplicity::Singleton);
        state.heap.write(
            &root_id,
            &format!("field{index}"),
            Field::Present(Value::leaf(Datum::Map {
                view: MapType::Interface,
                target: Some(object),
            })),
        );
    }
    // Dropping the arena isolates physical union sharing from the provenance-result memo.
    Ok(state.heap.freeze(&root))
}

#[test]
fn provenance_charges_ignore_union_sharing_in_objects_and_remainders() -> eyre::Result<()> {
    for first in 0..3 {
        for second in 0..3 {
            let warm = union_root(true, [first, second])?;
            let cold = union_root(false, [first, second])?;
            for remaining in 0..32 {
                sim_assert_eq!(have: observe(&warm, true, remaining), want: observe(&cold, true, remaining));
            }
        }
    }
    Ok(())
}

#[test]
fn terminal_object_provenance_matches_the_full_traversal_charge() -> eyre::Result<()> {
    for shape in 0..3 {
        let mut state = State::new(Value::nil());
        let inputs = Rc::new(BTreeSet::from([
            ValuesPath::parse("x"),
            ValuesPath::parse("y"),
        ]));
        let data = unknown_data(&state, inputs, shape)?;
        let object = state.heap.allocate(data, Multiplicity::Singleton);
        let root = state.heap.freeze(&Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: Some(object.clone()),
        }));
        let storage = root
            .heap
            .object(&object)
            .ok_or_eyre("terminal object missing")?;
        for remaining in 0..8 {
            *state.heap.arena.input_queries.borrow_mut() = InputCache::default();
            let mut budget = WorkBudget::new(remaining);
            let inputs = FrozenValue::object_inputs(&root.heap, &object, &storage, &mut budget);
            sim_assert_eq!(have: Observation { inputs, remaining: budget.remaining, exhausted: budget.is_exhausted() }, want: observe(&root, true, remaining));
        }
    }
    Ok(())
}
