//! Eager transfers of object contents into immutable store roots.

use std::collections::BTreeSet;
use std::rc::Rc;

use helm_schema_core::ValuesPath;

use super::heap_store::Store;
use super::work::WorkBudget;
use super::{
    Arena, FrozenTest, FrozenValue, HeapUpdate, HeapVersion, Multiplicity, ObjectData, ObjectId,
    ObjectVersion, Reach, Remainder, SnapshotId,
};

impl Store {
    pub(super) fn advance(
        parent: &Rc<HeapVersion>,
        node: &HeapUpdate,
        id: SnapshotId,
        arena: &Arena,
    ) -> Self {
        match node {
            HeapUpdate::Install { object, storage } => {
                let mut store = parent.store.clone();
                store.install(object, Rc::clone(storage));
                store
            }
            HeapUpdate::Write { object, key, field } => write_field(parent, object, key, field, id),
            HeapUpdate::WriteSlot {
                object,
                index,
                value,
            } => {
                let mut store = parent.store.clone();
                if let Some(previous) = store.object(object) {
                    let mut storage = previous.as_ref().clone();
                    storage.revision = id;
                    if let ObjectData::Strings(slots) = &mut storage.data
                        && let Some(slot) = slots.get_mut(*index)
                    {
                        *slot = value.clone();
                    }
                    store.install(object, Rc::new(storage));
                }
                store
            }
            HeapUpdate::UnknownWrite { objects, inputs } => {
                let mut store = parent.store.clone();
                match objects {
                    Reach::Top => store.invalidate(id, inputs),
                    Reach::Bounded(objects) => {
                        for object in objects.iter() {
                            if let Some(previous) = store.object(object) {
                                store.install(
                                    object,
                                    Rc::new(ObjectVersion {
                                        revision: id,
                                        multiplicity: previous.multiplicity,
                                        data: ObjectData::Widened {
                                            inputs: Rc::clone(inputs),
                                            // The writer may store any reached reference.
                                            reachable: Reach::Bounded(Rc::clone(objects)),
                                        },
                                    }),
                                );
                            }
                        }
                    }
                }
                store
            }
            HeapUpdate::Join { decision, yes, no } => {
                let paths = arena.input_paths.borrow();
                Store::join(&yes.store, &no.store, |object, left, right| {
                    let mut budget = WorkBudget::new(super::work::OBJECT_WORK);
                    join_object(
                        &JoinObject {
                            id,
                            arena,
                            paths: &paths,
                            decision,
                            yes,
                            no,
                            object,
                        },
                        left,
                        right,
                        &mut budget,
                    )
                })
            }
        }
    }
}

fn write_field(
    parent: &Rc<HeapVersion>,
    object: &ObjectId,
    key: &str,
    field: &super::Field,
    id: SnapshotId,
) -> Store {
    let mut store = parent.store.clone();
    if let Some(previous) = store.object(object) {
        let mut storage = previous.as_ref().clone();
        storage.revision = id;
        match &mut storage.data {
            ObjectData::Map {
                fields,
                remainder: _,
            } => {
                Rc::make_mut(fields).insert(key.to_owned(), field.clone());
                if fields.len() > 128 {
                    tracing::debug_span!(target: "helm_schema_ir::counters::heap_store", "object_widened", reason = "field_count").in_scope(|| {});
                    storage.data = ObjectData::Widened {
                        inputs: Rc::new(super::heap_snapshot::data_inputs(parent, &storage.data)),
                        reachable: super::heap::data_reach(&storage.data),
                    };
                }
            }
            ObjectData::Widened { inputs, reachable } => {
                storage.data = ObjectData::Map {
                    fields: Rc::new(super::ObjectFields::from_iter([(
                        key.to_owned(),
                        field.clone(),
                    )])),
                    remainder: super::layers::unknown_members(inputs, reachable),
                };
            }
            ObjectData::Strings(_) | ObjectData::Opaque { reachable: _ } => {}
        }
        store.install(object, Rc::new(storage));
    }
    store
}

struct JoinObject<'join> {
    id: SnapshotId,
    arena: &'join Arena,
    paths: &'join super::input_paths::InputPaths,
    decision: &'join Rc<FrozenTest>,
    yes: &'join Rc<HeapVersion>,
    no: &'join Rc<HeapVersion>,
    object: &'join ObjectId,
}

fn join_object(
    context: &JoinObject<'_>,
    yes_object: Option<Rc<ObjectVersion>>,
    no_object: Option<Rc<ObjectVersion>>,
    budget: &mut WorkBudget,
) -> Option<Rc<ObjectVersion>> {
    let JoinObject {
        id,
        arena,
        paths,
        decision,
        yes,
        no,
        object,
    } = *context;
    // Equality and construction share this object's allowance, before any deep comparison.
    if let Some((yes, no)) = yes_object.as_ref().zip(no_object.as_ref())
        && (Rc::ptr_eq(yes, no) || super::heap_equality::objects(yes, no, budget) == Some(true))
    {
        return yes_object;
    }
    let input_path = paths.get(object);
    let baseline = if yes_object.is_none() || no_object.is_none() {
        arena.derived.borrow().get(object).cloned().or_else(|| {
            input_path.map(|_| {
                Rc::new(ObjectVersion {
                    revision: id,
                    multiplicity: Multiplicity::Singleton,
                    data: ObjectData::Map {
                        fields: Rc::default(),
                        remainder: Remainder::Input,
                    },
                })
            })
        })
    } else {
        None
    };
    let yes_object = yes_object.or_else(|| baseline.clone());
    let no_object = no_object.or(baseline);
    match (yes_object, no_object) {
        (Some(yes_object), Some(no_object)) => {
            tracing::debug_span!(target: "helm_schema_ir::counters::heap_store", "heap_join_objects")
                .in_scope(|| {});
            let mut data = super::heap::join_data(
                decision,
                &yes_object.data,
                &no_object.data,
                Some(&arena.input_unions),
                budget,
            )
            .unwrap_or_else(|_| ObjectData::Widened {
                inputs: Rc::new(BTreeSet::from([ValuesPath::default()])),
                reachable: super::heap::data_reach(&yes_object.data)
                    .union(&super::heap::data_reach(&no_object.data)),
            });
            if budget.is_exhausted() {
                data = ObjectData::Widened {
                    inputs: Rc::new(BTreeSet::from([ValuesPath::default()])),
                    reachable: super::heap::data_reach(&yes_object.data)
                        .union(&super::heap::data_reach(&no_object.data)),
                };
            }
            if let ObjectData::Widened {
                inputs,
                reachable: _,
            } = &mut data
            {
                for (branch, storage) in [(yes, &yes_object), (no, &no_object)] {
                    if !matches!(
                        storage.data,
                        ObjectData::Widened {
                            inputs: _,
                            reachable: _,
                        }
                    ) {
                        let added = FrozenValue::object_inputs(branch, object, storage, budget);
                        if !added.is_subset(inputs) {
                            Rc::make_mut(inputs).extend(added);
                        }
                    }
                }
                if let Some(path) = input_path
                    && !inputs.contains(path)
                {
                    Rc::make_mut(inputs).insert(path.clone());
                }
                tracing::debug_span!(target: "helm_schema_ir::counters::heap_store", "object_widened", reason = if budget.is_exhausted() { "object_work" } else { "join_shape" }).in_scope(|| {});
            }
            Some(Rc::new(ObjectVersion {
                revision: id,
                multiplicity: yes_object.multiplicity.join(no_object.multiplicity),
                data,
            }))
        }
        (Some(storage), None) | (None, Some(storage)) => Some(storage),
        (None, None) => None,
    }
}

#[cfg(test)]
#[path = "../tests/runtime_heap_transfer_work.rs"]
mod tests;
