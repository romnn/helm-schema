use std::collections::BTreeSet;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::ValuesPath;
use imbl::ordmap::DiffItem;
use test_util::prelude::sim_assert_eq;

use super::StoredObject;
use crate::value::{Multiplicity, ObjectData, ObjectId, ObjectVersion, Remainder, State, Value};

#[test]
fn persistent_store_keeps_a_map_after_the_write_replay_budget() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    // Replaying this history needs more than 65,536 visits and field-copy charges.
    for _ in 0..25_000 {
        assert!(state.heap.write(
            &target,
            "x",
            crate::value::Field::Present(Value::string("exact"))
        ));
    }
    let frozen = state.heap.freeze(&map);
    // Removing the arena also removes the lazy query's memoized intermediate answers.
    drop(state);
    let value = frozen
        .read_field("x")
        .present()
        .ok_or_eyre("written member missing")?;
    sim_assert_eq!(have: value.exact_text(), want: Some("exact".to_owned()));
    Ok(())
}

#[test]
fn persistent_store_keeps_slots_after_the_slot_copy_budget() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let backing = state.heap.allocate(
        ObjectData::Strings(vec![Value::string("a"); 128]),
        Multiplicity::Singleton,
    );
    // Each replayed slot write copies 128 members and charges that work again.
    for _ in 0..600 {
        assert!(state.heap.write_slot(&backing, 0, Value::string("a")));
    }
    let frozen = state.heap.freeze(&Value::strings(backing.clone(), 128));
    drop(state);
    let storage = frozen
        .heap
        .object(&backing)
        .ok_or_eyre("string backing missing")?;
    sim_assert_eq!(have: &storage.data, want: &ObjectData::Strings(vec![Value::string("a"); 128]));
    Ok(())
}

#[test]
fn cached_subset_unions_preserve_complete_budget_charges() {
    use crate::value::heap::{InputUnions, join_input_sets};
    use crate::value::work::WorkBudget;
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
        let completed = join_input_sets(yes, no, Some(&cache), &mut WorkBudget::new(64));
        for limit in 0..8 {
            // Reusing a completed union cannot turn a truncated request into a complete proof.
            let mut cold = WorkBudget::new(limit);
            let mut warm = WorkBudget::new(limit);
            let expected = join_input_sets(yes, no, None, &mut cold);
            let have = join_input_sets(yes, no, Some(&cache), &mut warm);
            sim_assert_eq!(have: have, want: expected);
            sim_assert_eq!(have: warm.remaining, want: cold.remaining);
            sim_assert_eq!(have: warm.is_exhausted(), want: cold.is_exhausted());
        }
        sim_assert_eq!(have: completed, want: Rc::new(yes.union(no).cloned().collect::<BTreeSet<_>>()));
    }
}

#[test]
fn persistent_store_joins_shared_sequence_diamonds_without_expanding_them() -> eyre::Result<()> {
    use crate::value::{Coverage, Field};
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    let mut left = Value::string("same");
    let mut right = Value::string("same");
    for _ in 0..40 {
        left = Value::sequence(vec![left.clone(), left]);
        right = Value::sequence(vec![right.clone(), right]);
    }
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    assert!(yes.write(&target, "value", Field::Present(left.clone())));
    assert!(no.write(&target, "value", Field::Present(right)));
    let condition = helm_schema_core::Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    // Equal shared graphs collapse to the original value, without an expanded-tree walk.
    let stored = state
        .heap
        .version
        .store
        .object(&target)
        .ok_or_eyre("joined map missing")?;
    let ObjectData::Map {
        fields,
        remainder: _,
    } = &stored.data
    else {
        eyre::bail!("joined map widened");
    };
    let Some(Field::Present(value)) = fields.get("value") else {
        eyre::bail!("joined member missing");
    };
    assert!(Rc::ptr_eq(&value.node, &left.node));
    Ok(())
}

#[test]
fn persistent_store_havoc_releases_unobserved_snapshot_roots() -> eyre::Result<()> {
    use crate::value::{Field, SnapshotId};
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    let frozen = state.heap.freeze(&map);
    let old_root = Rc::downgrade(&frozen.heap);
    let captured = Value::frozen_text(
        crate::value::text::TextTerm::Yaml {
            value: frozen,
            pretty: false,
        },
        BTreeSet::new(),
    );
    let mut store = crate::value::heap_store::Store::default();
    store.install(
        &target,
        Rc::new(ObjectVersion {
            revision: SnapshotId { ordinal: 100 },
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Map {
                fields: Rc::new(crate::value::ObjectFields::from_iter([(
                    "saved".to_owned(),
                    Field::Present(captured),
                )])),
                remainder: Remainder::Closed,
            },
        }),
    );
    drop(state);
    assert!(old_root.upgrade().is_some());
    store.invalidate(SnapshotId { ordinal: 101 }, &Rc::new(BTreeSet::new()));
    // A live identity does not keep discarded contents or the snapshots inside them alive.
    assert!(old_root.upgrade().is_none());
    sim_assert_eq!(have: store.len(), want: 1);
    Ok(())
}

/// Preserves the sparse-membership and summary-promotion laws of `runtime_heap_headers`.
#[test]
fn persistent_store_keeps_sparse_members_and_promotes_only_shared_summaries() -> eyre::Result<()> {
    use crate::value::{Coverage, ObjectId};
    let mut state = State::new(Value::nil());
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    for (heap, ordinals, multiplicity) in [
        (&mut yes, vec![0, 7, 64, 1 << 40], Multiplicity::Singleton),
        (&mut no, vec![7, 65, u64::MAX], Multiplicity::Summary),
    ] {
        for ordinal in ordinals {
            heap.install(
                ObjectId { ordinal },
                ObjectVersion {
                    revision: heap.arena.snapshot(),
                    multiplicity,
                    data: ObjectData::Map {
                        fields: Rc::default(),
                        remainder: Remainder::Closed,
                    },
                },
            );
        }
    }
    let predicate = helm_schema_core::Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    let expected = [
        (0, Multiplicity::Singleton),
        (7, Multiplicity::Summary),
        (64, Multiplicity::Singleton),
        (65, Multiplicity::Summary),
        (1 << 40, Multiplicity::Singleton),
        (u64::MAX, Multiplicity::Summary),
    ];
    sim_assert_eq!(have: state.heap.version.store.objects(), want: expected.iter().map(|(ordinal, _)| ObjectId { ordinal: *ordinal }).collect::<Vec<_>>());
    for (ordinal, multiplicity) in expected {
        let object = ObjectId { ordinal };
        sim_assert_eq!(have: state.heap.version.store.multiplicity(&object), want: Some(multiplicity));
        let stored = state
            .heap
            .version
            .object(&object)
            .ok_or_eyre("joined object missing")?;
        sim_assert_eq!(have: stored.multiplicity, want: multiplicity);
    }
    for ordinal in [1, 63, 66] {
        sim_assert_eq!(have: state.heap.version.store.multiplicity(&ObjectId { ordinal }), want: None);
    }
    Ok(())
}

/// Preserves hot frozen-ancestor and borrowed-view laws without an evictable object cache.
#[test]
fn persistent_store_borrowed_fields_survive_other_versions_and_owner_reclamation()
-> eyre::Result<()> {
    use crate::value::Field;
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .write(&target, "saved", Field::Present(Value::string("before")));
    let frozen = state.heap.freeze(&map);
    let borrowed = frozen
        .heap
        .borrowed_object(&target)
        .ok_or_eyre("borrowed map missing")?;
    state
        .heap
        .write(&target, "saved", Field::Present(Value::string("after")));
    drop(state);
    let again = frozen
        .heap
        .borrowed_object(&target)
        .ok_or_eyre("borrowed map expired")?;
    assert!(std::ptr::eq(borrowed, again));
    let ObjectData::Map {
        fields,
        remainder: _,
    } = &borrowed.data
    else {
        eyre::bail!("borrowed map widened");
    };
    sim_assert_eq!(have: fields.get("saved"), want: Some(&Field::Present(Value::string("before"))));
    Ok(())
}

#[test]
fn persistent_store_reclaims_deep_acyclic_snapshot_chains_iteratively() -> eyre::Result<()> {
    use crate::value::{Field, Heap};
    let mut heap = Heap::new();
    let map = heap.map(Remainder::Closed);
    let target = heap
        .version
        .store
        .objects()
        .into_iter()
        .next()
        .ok_or_eyre("map missing")?;
    let live = Rc::clone(&heap.arena.live_versions);
    let mut observed = Vec::new();
    for index in 0..10_000 {
        let frozen = heap.freeze(&map);
        if index % 1000 == 0 {
            observed.push(Rc::downgrade(&frozen.heap));
        }
        let text = Value::frozen_text(
            crate::value::text::TextTerm::Yaml {
                value: frozen,
                pretty: false,
            },
            BTreeSet::new(),
        );
        assert!(heap.write(&target, "previous", Field::Present(text)));
    }
    // Each stored serialization owns its older snapshot, but none owns the live arena.
    drop(heap);
    assert!(observed.iter().all(|root| root.upgrade().is_none()));
    sim_assert_eq!(have: live.get(), want: 0);
    Ok(())
}

#[test]
fn persistent_store_summary_writes_preserve_equal_shared_sequence_graphs() -> eyre::Result<()> {
    use crate::value::Field;
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    let mut before = Value::string("same");
    let mut after = Value::string("same");
    for _ in 0..40 {
        before = Value::sequence(vec![before.clone(), before]);
        after = Value::sequence(vec![after.clone(), after]);
    }
    state.heap.install(
        target.clone(),
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Summary,
            data: ObjectData::Map {
                fields: Rc::new(crate::value::ObjectFields::from_iter([(
                    "value".to_owned(),
                    Field::Present(before),
                )])),
                remainder: Remainder::Closed,
            },
        },
    );
    assert!(
        state
            .heap
            .write(&target, "value", Field::Present(after.clone()))
    );
    let storage = state
        .heap
        .version
        .object(&target)
        .ok_or_eyre("summary map missing")?;
    let ObjectData::Map {
        fields,
        remainder: _,
    } = &storage.data
    else {
        eyre::bail!("summary map widened");
    };
    let Some(Field::Present(value)) = fields.get("value") else {
        eyre::bail!("equal summary write gained a choice");
    };
    assert!(Rc::ptr_eq(&value.node, &after.node));
    Ok(())
}

#[test]
fn persistent_store_summary_field_retention_widens_only_its_object() -> eyre::Result<()> {
    use crate::value::Field;
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    let other = state.heap.map(Remainder::Closed);
    let other_target = state.map_target(&other).ok_or_eyre("other map missing")?;
    state.heap.write(
        &other_target,
        "kept",
        Field::Present(Value::string("exact")),
    );
    state.heap.install(
        target.clone(),
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Summary,
            data: ObjectData::Map {
                fields: Rc::default(),
                remainder: Remainder::Closed,
            },
        },
    );
    let mut widened = false;
    for index in 0..256 {
        let field = if index % 2 == 0 {
            Field::Present(Value::string("set"))
        } else {
            Field::Absent
        };
        assert!(state.heap.write(&target, "value", field));
        let stored = state
            .heap
            .version
            .object(&target)
            .ok_or_eyre("summary map missing")?;
        if matches!(stored.data, ObjectData::Widened { inputs: _ }) {
            widened = true;
            break;
        }
    }
    assert!(
        widened,
        "summary field retention exceeded its local fanout allowance"
    );
    sim_assert_eq!(have: state.heap.version.field(&other_target, "kept"), want: Field::Present(Value::string("exact")));
    Ok(())
}

#[test]
fn persistent_store_coverage_diff_visits_only_changed_explicit_objects() -> eyre::Result<()> {
    use crate::value::Field;
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    let others = (0..31)
        .map(|_| state.heap.map(Remainder::Closed))
        .collect::<Vec<_>>();
    let before = Rc::clone(&state.heap.version);
    assert!(
        state
            .heap
            .write(&target, "kept", Field::Present(Value::string("exact")))
    );
    let current = Rc::clone(&state.heap.version);
    let mut visited = BTreeSet::new();
    current
        .store
        .visit_explicit_since(&before.store, |object, _storage| {
            visited.insert(object);
        });
    sim_assert_eq!(have: visited, want: BTreeSet::from([target.clone()]));
    let inputs = Rc::new(BTreeSet::from([ValuesPath::parse("unknown")]));
    state.heap.unknown_write(Rc::clone(&inputs));
    let epoch = Rc::clone(&state.heap.version);
    let mut visited = BTreeSet::new();
    epoch
        .store
        .visit_explicit_since(&current.store, |object, _storage| {
            visited.insert(object);
        });
    sim_assert_eq!(have: visited, want: BTreeSet::new());
    sim_assert_eq!(have: epoch.store.len(), want: others.len() + 1);
    sim_assert_eq!(have: epoch.store.fallback_inputs(), want: Some(&inputs));
    assert!(
        state
            .heap
            .write(&target, "later", Field::Present(Value::string("value")))
    );
    let written = Rc::clone(&state.heap.version);
    let mut visited = BTreeSet::new();
    // Backward traversal removes the override; its contribution comes from the epoch.
    epoch
        .store
        .visit_explicit_since(&written.store, |object, _storage| {
            visited.insert(object);
        });
    sim_assert_eq!(have: visited, want: BTreeSet::new());
    assert!(epoch.store.is_fallback(&target));
    let fallback = epoch.store.object(&target).ok_or_eyre("fallback missing")?;
    sim_assert_eq!(have: &fallback.data, want: &ObjectData::Widened { inputs });
    Ok(())
}

#[test]
fn persistent_store_explicit_and_epoch_views_match_sparse_point_lookups() -> eyre::Result<()> {
    use crate::value::heap_store::Store;
    use crate::value::{ObjectId, SnapshotId};

    fn check(store: &Store) -> eyre::Result<()> {
        let mut actual = std::collections::BTreeMap::new();
        store.visit_explicit(|object, storage| {
            actual.insert(object, storage.data.clone());
        });
        for object in store.objects() {
            if store.is_fallback(&object) {
                let inputs = store.fallback_inputs().ok_or_eyre("epoch inputs missing")?;
                actual.insert(
                    object,
                    ObjectData::Widened {
                        inputs: Rc::clone(inputs),
                    },
                );
            }
        }
        let mut expected = std::collections::BTreeMap::new();
        for object in store.objects() {
            let storage = store
                .object(&object)
                .ok_or_eyre("allocated object missing")?;
            expected.insert(object, storage.data.clone());
        }
        sim_assert_eq!(have: actual, want: expected);
        Ok(())
    }

    let mut store = Store::default();
    check(&store)?;
    for ordinal in [0_u64, 7, 64, 65, 1 << 40, u64::MAX] {
        store.install(
            &ObjectId { ordinal },
            Rc::new(ObjectVersion {
                revision: SnapshotId { ordinal },
                multiplicity: if ordinal == 65 {
                    Multiplicity::Summary
                } else {
                    Multiplicity::Singleton
                },
                data: ObjectData::Map {
                    fields: Rc::default(),
                    remainder: Remainder::Closed,
                },
            }),
        );
    }
    check(&store)?;
    store.invalidate(
        SnapshotId { ordinal: 100 },
        &Rc::new(BTreeSet::from([ValuesPath::parse("unknown")])),
    );
    check(&store)?;
    // Middle and distant overrides leave fallback identities on both sides; one identity is new.
    for ordinal in [7, 256, 1 << 40] {
        store.install(
            &ObjectId { ordinal },
            Rc::new(ObjectVersion {
                revision: SnapshotId { ordinal: 200 },
                multiplicity: Multiplicity::Summary,
                data: ObjectData::Strings(vec![Value::string("override")]),
            }),
        );
    }
    check(&store)?;
    for object in store.objects() {
        store.install(
            &object,
            Rc::new(ObjectVersion {
                revision: SnapshotId { ordinal: 300 },
                multiplicity: Multiplicity::Singleton,
                data: ObjectData::Strings(Vec::new()),
            }),
        );
    }
    check(&store)?;
    sim_assert_eq!(have: store.fallback_inputs(), want: None);
    Ok(())
}

impl super::Store {
    pub(in crate::value) fn visit_explicit(
        &self,
        mut visit: impl FnMut(ObjectId, &Rc<ObjectVersion>),
    ) {
        for (object, StoredObject { version }) in &self.contents {
            visit(object.clone(), version);
        }
    }

    pub(in crate::value) fn visit_explicit_since(
        &self,
        previous: &Self,
        mut visit: impl FnMut(ObjectId, &Rc<ObjectVersion>),
    ) {
        for difference in self.contents.diff(&previous.contents) {
            let object = match difference {
                DiffItem::Add(object, _)
                | DiffItem::Remove(object, _)
                | DiffItem::Update {
                    old: (object, _),
                    new: _,
                } => object,
            };
            if let Some(StoredObject { version }) = self.contents.get(object) {
                visit(object.clone(), version);
            }
        }
    }

    pub(in crate::value) fn fallback_inputs(&self) -> Option<&Rc<BTreeSet<ValuesPath>>> {
        if self.contents.len() < self.identities.len() {
            self.epoch.as_ref().map(|epoch| &epoch.inputs)
        } else {
            None
        }
    }

    pub(in crate::value) fn is_fallback(&self, object: &ObjectId) -> bool {
        self.epoch.is_some()
            && !self.contents.contains_key(object)
            && self.identities.contains_key(object)
    }

    pub(in crate::value) fn visit(
        &self,
        mut visit: impl FnMut(crate::value::ObjectId, &Rc<ObjectVersion>),
    ) {
        for object in self.identities.keys() {
            if let Some(storage) = self.lookup(object) {
                visit(object.clone(), storage);
            }
        }
    }
}
