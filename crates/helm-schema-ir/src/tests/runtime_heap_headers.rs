use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use super::AllocationIndex;
use crate::value::heap_query::QueryBudget;
use crate::value::{
    Coverage, Heap, Multiplicity, ObjectData, ObjectId, ObjectVersion, Remainder, State, Value,
};

fn install(heap: &mut Heap, ordinal: u64, multiplicity: Multiplicity) {
    heap.install(
        ObjectId { ordinal },
        ObjectVersion {
            revision: heap.arena.snapshot(),
            multiplicity,
            data: ObjectData::Map {
                fields: Rc::new(BTreeMap::new()),
                remainder: Remainder::Closed,
            },
        },
    );
}

#[test]
fn sparse_branch_membership_and_summary_promotion_are_exact() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    for ordinal in [0, 7, 64, 1 << 40] {
        install(&mut yes, ordinal, Multiplicity::Singleton);
    }
    for ordinal in [7, 65, u64::MAX] {
        install(&mut no, ordinal, Multiplicity::Summary);
    }
    sim_assert_eq!(have: yes.version.allocations.get(&ObjectId { ordinal: 65 }), want: None);
    sim_assert_eq!(have: no.version.allocations.get(&ObjectId { ordinal: 64 }), want: None);
    let condition = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    let expected = BTreeMap::from([
        (ObjectId { ordinal: 0 }, Multiplicity::Singleton),
        (ObjectId { ordinal: 7 }, Multiplicity::Summary),
        (ObjectId { ordinal: 64 }, Multiplicity::Singleton),
        (ObjectId { ordinal: 65 }, Multiplicity::Summary),
        (ObjectId { ordinal: 1 << 40 }, Multiplicity::Singleton),
        (ObjectId { ordinal: u64::MAX }, Multiplicity::Summary),
    ]);
    sim_assert_eq!(have: state.heap.version.object_count(), want: expected.len());
    for (object, multiplicity) in &expected {
        sim_assert_eq!(have: state.heap.version.allocations.get(object), want: Some(*multiplicity));
        let storage = state
            .heap
            .version
            .object(object)
            .ok_or_eyre("joined object missing")?;
        sim_assert_eq!(have: storage.multiplicity, want: *multiplicity);
        sim_assert_eq!(have: &storage.data, want: &ObjectData::Map {
            fields: Rc::new(BTreeMap::new()), remainder: Remainder::Closed,
        });
    }
    sim_assert_eq!(have: state.heap.version.allocations.get(&ObjectId { ordinal: 63 }), want: None);
    sim_assert_eq!(have: state.heap.version.query_object_ids(&mut QueryBudget::new(1024)), want: Some(expected.into_keys().collect::<Vec<_>>()));
    Ok(())
}

#[test]
fn allocation_enumeration_has_an_explicit_cap_neighbor() -> eyre::Result<()> {
    let empty = AllocationIndex::default();
    sim_assert_eq!(have: empty.objects(&mut QueryBudget::new(0)), want: None);
    sim_assert_eq!(have: empty.objects(&mut QueryBudget::new(1)), want: Some(Vec::new()));
    let mut state = State::new(Value::nil());
    install(&mut state.heap, u64::MAX, Multiplicity::Singleton);
    let work = usize::try_from(u64::BITS)? + 2;
    sim_assert_eq!(have: state.heap.version.query_object_ids(&mut QueryBudget::new(work - 1)), want: None);
    sim_assert_eq!(have: state.heap.version.query_object_ids(&mut QueryBudget::new(work)), want: Some(vec![ObjectId { ordinal: u64::MAX }]));
    Ok(())
}

#[test]
fn replacing_contents_reuses_only_the_allocation_index() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    install(&mut state.heap, 0, Multiplicity::Singleton);
    let before = state
        .heap
        .version
        .allocations
        .root
        .clone()
        .ok_or_eyre("allocation root missing")?;
    let object = ObjectId { ordinal: 0 };
    state.heap.install(
        object.clone(),
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Widened {
                inputs: Rc::new(BTreeSet::new()),
            },
        },
    );
    let after = state
        .heap
        .version
        .allocations
        .root
        .as_ref()
        .ok_or_eyre("allocation root missing")?;
    sim_assert_eq!(have: Rc::ptr_eq(&before, after), want: true);
    let storage = state
        .heap
        .version
        .object(&object)
        .ok_or_eyre("replacement missing")?;
    sim_assert_eq!(have: &storage.data, want: &ObjectData::Widened { inputs: Rc::new(BTreeSet::new()) });
    install(&mut state.heap, 0, Multiplicity::Summary);
    sim_assert_eq!(have: state.heap.version.allocations.get(&object), want: Some(Multiplicity::Summary));
    sim_assert_eq!(have: state.heap.version.object_count(), want: 1);
    Ok(())
}

#[test]
fn singleton_prefix_preserves_holes_and_branch_summary_promotion() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    for ordinal in [0, 3, 2, 1, 4] {
        install(&mut state.heap, ordinal, Multiplicity::Singleton);
    }
    let mut yes = state.heap.fork();
    let no = state.heap.fork();
    install(&mut yes, 2, Multiplicity::Summary);
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    let actual = (0..=5)
        .map(|ordinal| state.heap.version.allocations.get(&ObjectId { ordinal }))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: actual, want: vec![
        Some(Multiplicity::Singleton), Some(Multiplicity::Singleton),
        Some(Multiplicity::Summary), Some(Multiplicity::Singleton),
        Some(Multiplicity::Singleton), None,
    ]);
    Ok(())
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct LogicalPrefix {
    remaining: u32,
    ordinal: u64,
}

fn legacy_enumeration_work(objects: &BTreeSet<ObjectId>) -> usize {
    let height = objects
        .iter()
        .map(|object| object.ordinal.bit_width())
        .max()
        .unwrap_or(0);
    let mut prefixes = BTreeSet::new();
    for object in objects {
        for remaining in 0..=height {
            // The logical root above all 64 ordinal bits has prefix zero.
            prefixes.insert(LogicalPrefix {
                remaining,
                ordinal: object.ordinal.checked_shr(remaining).unwrap_or(0),
            });
        }
    }
    1 + prefixes.len()
}

#[test]
fn bitmap_pages_spend_the_exact_legacy_prefix_allowance() {
    let cases = [
        Vec::new(),
        vec![0],
        vec![1],
        vec![63],
        (0..64).collect(),
        (0..128).collect(),
        vec![u64::MAX, 128, 1, 64, 0, 63],
        vec![
            0,
            1,
            31,
            32,
            63,
            64,
            65,
            127,
            128,
            255,
            (1 << 40) + 63,
            u64::MAX,
        ],
    ];
    for ordinals in cases {
        let mut state = State::new(Value::nil());
        for ordinal in &ordinals {
            install(&mut state.heap, *ordinal, Multiplicity::Singleton);
        }
        let expected = ordinals
            .into_iter()
            .map(|ordinal| ObjectId { ordinal })
            .collect::<BTreeSet<_>>();
        let work = legacy_enumeration_work(&expected);
        let expected = expected.into_iter().collect::<Vec<_>>();
        // Compressed pages preserve both the failure neighbor and the exact successful cost.
        sim_assert_eq!(have: state.heap.version.allocations.objects(&mut QueryBudget::new(work - 1)), want: None);
        let mut budget = QueryBudget::new(work + 11);
        sim_assert_eq!(have: state.heap.version.allocations.objects(&mut budget), want: Some(expected));
        sim_assert_eq!(have: budget.remaining, want: 11);
    }
}

#[test]
fn mixed_bitmap_pages_join_sparse_ids_and_promote_summaries_without_losing_members()
-> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let mut expected = BTreeMap::new();
    for ordinal in (0..65).chain([127, 128, 4096, 1 << 40, u64::MAX - 63, u64::MAX]) {
        install(&mut state.heap, ordinal, Multiplicity::Singleton);
        expected.insert(ObjectId { ordinal }, Multiplicity::Singleton);
    }
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    for ordinal in [1, 63, 128, u64::MAX] {
        install(&mut yes, ordinal, Multiplicity::Summary);
        expected.insert(ObjectId { ordinal }, Multiplicity::Summary);
    }
    for ordinal in [65, 4097, u64::MAX - 1] {
        install(&mut no, ordinal, Multiplicity::Singleton);
        expected.insert(ObjectId { ordinal }, Multiplicity::Singleton);
    }
    install(&mut no, 64, Multiplicity::Summary);
    expected.insert(ObjectId { ordinal: 64 }, Multiplicity::Summary);
    let predicate = Predicate::truthy_path("bitmap_join");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    sim_assert_eq!(have: state.heap.version.allocations.len(), want: expected.len());
    for (object, multiplicity) in &expected {
        sim_assert_eq!(have: state.heap.version.allocations.get(object), want: Some(*multiplicity));
    }
    for ordinal in [66, 126, 129, 4098, u64::MAX - 2] {
        sim_assert_eq!(have: state.heap.version.allocations.get(&ObjectId { ordinal }), want: None);
    }
    let expected = expected.into_keys().collect::<BTreeSet<_>>();
    let work = legacy_enumeration_work(&expected);
    let expected = expected.into_iter().collect::<Vec<_>>();
    sim_assert_eq!(have: state.heap.version.allocations.objects(&mut QueryBudget::new(work - 1)), want: None);
    sim_assert_eq!(have: state.heap.version.allocations.objects(&mut QueryBudget::new(work)), want: Some(expected));
    Ok(())
}
