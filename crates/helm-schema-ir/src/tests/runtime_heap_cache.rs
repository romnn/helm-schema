use std::collections::BTreeSet;
use std::rc::Rc;

use color_eyre::eyre;
use helm_schema_core::ValuesPath;
use test_util::prelude::sim_assert_eq;

use super::{CacheLimits, MAX_PAYLOAD, ObjectCache, ObjectLookup, WidenedCache};
use crate::value::heap_query::ObjectQueryKey;
use crate::value::{Multiplicity, ObjectData, ObjectVersion, SnapshotId};

#[test]
fn shared_widened_provenance_occupies_one_payload() {
    let inputs = Rc::new(
        (0..1024)
            .map(|index| ValuesPath::parse(&format!("input{index}")))
            .collect::<BTreeSet<_>>(),
    );
    let object = Rc::new(ObjectVersion {
        revision: SnapshotId { ordinal: 1 },
        multiplicity: Multiplicity::Singleton,
        data: ObjectData::Widened {
            inputs: Rc::clone(&inputs),
        },
    });
    let mut cache = ObjectCache::default();
    for version in 0..512 {
        cache.insert(ObjectQueryKey { version, object: 0 }, Some(&object));
    }
    sim_assert_eq!(have: cache.entries.len(), want: 512);
    sim_assert_eq!(have: cache.inputs.len(), want: 1);
    sim_assert_eq!(have: cache.payload, want: inputs.len() + 1 + 512);
}

#[test]
fn a_hot_frozen_ancestor_survives_newer_snapshots() -> eyre::Result<()> {
    let limit = 64;
    let mut cache = ObjectCache {
        limits: CacheLimits {
            entries: limit,
            payload: MAX_PAYLOAD,
        },
        ..ObjectCache::default()
    };
    let hot = ObjectQueryKey {
        version: 0,
        object: 0,
    };
    cache.insert(hot, None);
    for version in 1..=(u64::try_from(limit)? + 64) {
        sim_assert_eq!(have: cache.get(&hot), want: ObjectLookup::Hit(None));
        cache.insert(ObjectQueryKey { version, object: 0 }, None);
    }
    sim_assert_eq!(have: cache.entries.len(), want: limit);
    sim_assert_eq!(have: cache.get(&hot), want: ObjectLookup::Hit(None));
    sim_assert_eq!(have: cache.get(&ObjectQueryKey { version: 1, object: 0 }), want: ObjectLookup::Miss);
    Ok(())
}

#[test]
fn eviction_order_stays_lazy_and_preserves_insertion_order() {
    let mut cache = ObjectCache {
        limits: CacheLimits {
            entries: 3,
            payload: MAX_PAYLOAD,
        },
        ..ObjectCache::default()
    };
    for version in [90, 1, 50] {
        cache.insert(ObjectQueryKey { version, object: 0 }, None);
    }
    sim_assert_eq!(have: cache.order.is_none(), want: true);
    cache.insert(
        ObjectQueryKey {
            version: 7,
            object: 0,
        },
        None,
    );
    sim_assert_eq!(have: cache.order.is_some(), want: true);
    sim_assert_eq!(have: cache.get(&ObjectQueryKey { version: 90, object: 0 }), want: ObjectLookup::Miss);
    for version in [1, 50, 7] {
        sim_assert_eq!(have: cache.get(&ObjectQueryKey { version, object: 0 }), want: ObjectLookup::Hit(None));
    }
}

#[test]
fn payload_cap_neighbors_release_only_the_last_shared_input_owner() {
    let inputs = Rc::new(BTreeSet::from([ValuesPath::parse("x")]));
    let storage = Rc::new(ObjectVersion {
        revision: SnapshotId { ordinal: 0 },
        multiplicity: Multiplicity::Singleton,
        data: ObjectData::Widened {
            inputs: Rc::clone(&inputs),
        },
    });
    let mut cache = ObjectCache {
        limits: CacheLimits {
            entries: 3,
            payload: 4,
        },
        ..ObjectCache::default()
    };
    for version in 0..3 {
        cache.insert(ObjectQueryKey { version, object: 0 }, Some(&storage));
    }
    sim_assert_eq!(have: cache.entries.len(), want: 2);
    sim_assert_eq!(have: cache.payload, want: 4);
    sim_assert_eq!(have: cache.inputs.values().map(|entry| entry.references).collect::<Vec<_>>(), want: vec![2]);
    cache.insert(
        ObjectQueryKey {
            version: 3,
            object: 0,
        },
        None,
    );
    cache.insert(
        ObjectQueryKey {
            version: 4,
            object: 0,
        },
        None,
    );
    cache.insert(
        ObjectQueryKey {
            version: 5,
            object: 0,
        },
        None,
    );
    sim_assert_eq!(have: cache.entries.len(), want: 3);
    sim_assert_eq!(have: cache.payload, want: 3);
    sim_assert_eq!(have: cache.inputs.len(), want: 0);
    let oversized = Rc::new(ObjectVersion {
        revision: SnapshotId { ordinal: 1 },
        multiplicity: Multiplicity::Singleton,
        data: ObjectData::Widened {
            inputs: Rc::new(BTreeSet::from([
                ValuesPath::parse("x"),
                ValuesPath::parse("y"),
                ValuesPath::parse("z"),
            ])),
        },
    });
    cache.insert(
        ObjectQueryKey {
            version: 6,
            object: 0,
        },
        Some(&oversized),
    );
    sim_assert_eq!(have: cache.entries.len(), want: 3);
    sim_assert_eq!(have: cache.payload, want: 3);
    sim_assert_eq!(have: cache.get(&ObjectQueryKey { version: 6, object: 0 }), want: ObjectLookup::Miss);
}

#[test]
fn widened_views_share_only_identical_version_multiplicity_and_provenance() {
    let mut cache = WidenedCache::default();
    let inputs = Rc::new(BTreeSet::from([ValuesPath::parse("x")]));
    let first = cache.object(SnapshotId { ordinal: 1 }, Multiplicity::Singleton, &inputs);
    let same = cache.object(SnapshotId { ordinal: 1 }, Multiplicity::Singleton, &inputs);
    sim_assert_eq!(have: Rc::ptr_eq(&first, &same), want: true);
    let other_version = cache.object(SnapshotId { ordinal: 2 }, Multiplicity::Singleton, &inputs);
    sim_assert_eq!(have: Rc::ptr_eq(&first, &other_version), want: false);
    let other_multiplicity =
        cache.object(SnapshotId { ordinal: 1 }, Multiplicity::Summary, &inputs);
    sim_assert_eq!(have: Rc::ptr_eq(&first, &other_multiplicity), want: false);
    let other_inputs = Rc::new(BTreeSet::from([ValuesPath::parse("y")]));
    let other_source = cache.object(
        SnapshotId { ordinal: 1 },
        Multiplicity::Singleton,
        &other_inputs,
    );
    sim_assert_eq!(have: &other_source.data, want: &ObjectData::Widened { inputs: other_inputs });
    let expired = Rc::downgrade(&first);
    drop(first);
    drop(same);
    sim_assert_eq!(have: expired.upgrade(), want: None);
}
