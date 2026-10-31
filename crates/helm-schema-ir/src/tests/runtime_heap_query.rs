use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::ValuesPath;
use test_util::prelude::sim_assert_eq;

use crate::value::{Multiplicity, ObjectData, ObjectVersion, Remainder, State, Value};

#[test]
fn exhausted_borrowed_views_remain_unknown_and_do_not_replace_completed_answers() -> eyre::Result<()>
{
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    let expected = ObjectVersion {
        revision: state.heap.arena.snapshot(),
        multiplicity: Multiplicity::Singleton,
        data: ObjectData::Map {
            fields: Rc::new(BTreeMap::new()),
            remainder: Remainder::Closed,
        },
    };
    state.heap.install(target.clone(), expected.clone());
    let root = &state.heap.version;

    // Missing capacity is unknown storage, so provenance queries cannot infer absence.
    root.borrowed_objects.retained.set(4096);
    let unknown = root
        .borrowed_object(&target)
        .ok_or_eyre("unknown view missing")?;
    sim_assert_eq!(have: unknown, want: &ObjectVersion {
        revision: root.id,
        multiplicity: Multiplicity::Summary,
        data: ObjectData::Widened { inputs: Rc::new(BTreeSet::from([ValuesPath::default()])) },
    });

    // A completed request still obtains the unchanged canonical storage.
    root.borrowed_objects.retained.set(0);
    sim_assert_eq!(have: root.borrowed_object(&target), want: Some(&expected));
    Ok(())
}

#[test]
fn cached_subset_unions_preserve_complete_budget_charges() {
    use crate::value::heap::{InputUnions, join_input_sets};
    use crate::value::heap_query::QueryBudget;
    use std::cell::RefCell;

    let superset = Rc::new(BTreeSet::from([
        ValuesPath::parse("a"),
        ValuesPath::parse("b"),
    ]));
    let subset = Rc::new(BTreeSet::from([ValuesPath::parse("a")]));
    let other = Rc::new(BTreeSet::from([ValuesPath::parse("c")]));
    for (yes, no) in [
        (&superset, &subset),
        (&subset, &superset),
        (&superset, &other),
    ] {
        let cache = RefCell::new(InputUnions::default());
        let completed = join_input_sets(yes, no, Some(&cache), &mut QueryBudget::new(64));
        for limit in 0..8 {
            // Reusing a completed union cannot turn a truncated request into a complete proof.
            let mut cold = QueryBudget::new(limit);
            let mut warm = QueryBudget::new(limit);
            let expected = join_input_sets(yes, no, None, &mut cold);
            let have = join_input_sets(yes, no, Some(&cache), &mut warm);
            sim_assert_eq!(have: have, want: expected);
            sim_assert_eq!(have: warm.remaining, want: cold.remaining);
            sim_assert_eq!(have: warm.is_exhausted(), want: cold.is_exhausted());
        }
        sim_assert_eq!(have: completed, want: Rc::new(yes.union(no).cloned().collect::<BTreeSet<_>>()));
    }
}
