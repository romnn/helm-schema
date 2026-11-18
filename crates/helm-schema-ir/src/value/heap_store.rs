//! Persistent object contents with path copying and epoch-based global invalidation.

use std::collections::BTreeSet;
use std::rc::Rc;

use helm_schema_core::ValuesPath;
use imbl::GenericOrdMap;
use imbl::ordmap::DiffItem;
use imbl::shared_ptr::RcK;

use super::heap_regions::{ObjectRegions, Projection};
use super::{Multiplicity, ObjectData, ObjectId, ObjectVersion, SnapshotId};

#[derive(Clone, Debug)]
struct Epoch {
    inputs: Rc<BTreeSet<ValuesPath>>,
    singleton: Rc<ObjectVersion>,
    summary: Rc<ObjectVersion>,
}

impl Epoch {
    fn object(&self, multiplicity: Multiplicity) -> &Rc<ObjectVersion> {
        match multiplicity {
            Multiplicity::Singleton => &self.singleton,
            Multiplicity::Summary => &self.summary,
        }
    }
}

#[derive(Clone, Debug)]
struct StoredObject {
    version: Rc<ObjectVersion>,
}

impl PartialEq for StoredObject {
    fn eq(&self, other: &Self) -> bool {
        let Self { version } = self;
        let Self {
            version: other_version,
        } = other;
        Rc::ptr_eq(version, other_version)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Store {
    identities: GenericOrdMap<ObjectId, Multiplicity, RcK>,
    contents: GenericOrdMap<ObjectId, StoredObject, RcK>,
    epoch: Option<Rc<Epoch>>,
    projection: Projection,
}

impl Store {
    pub(super) fn multiplicity(&self, object: &ObjectId) -> Option<Multiplicity> {
        self.identities.get(object).copied()
    }

    pub(super) fn len(&self) -> usize {
        self.identities.len()
    }

    pub(super) fn borrowed(&self, object: &ObjectId) -> Option<&ObjectVersion> {
        self.lookup(object).map(Rc::as_ref)
    }

    pub(super) fn object(&self, object: &ObjectId) -> Option<Rc<ObjectVersion>> {
        self.lookup(object).cloned()
    }

    fn lookup(&self, object: &ObjectId) -> Option<&Rc<ObjectVersion>> {
        if let Some(storage) = self.contents.get(object) {
            return Some(&storage.version);
        }
        let multiplicity = self.identities.get(object)?;
        self.epoch.as_ref().map(|epoch| epoch.object(*multiplicity))
    }

    pub(super) fn install(&mut self, object: &ObjectId, storage: Rc<ObjectVersion>) {
        let current = self.lookup(object);
        if current.is_some_and(|current| Rc::ptr_eq(current, &storage)) {
            return;
        }
        let before = current
            .map(|current| ObjectRegions::from(&current.data))
            .unwrap_or_default();
        self.projection.install(
            object,
            storage.multiplicity,
            before,
            ObjectRegions::from(&storage.data),
        );
        if self.identities.get(object) != Some(&storage.multiplicity) {
            self.identities.insert(object.clone(), storage.multiplicity);
        }
        self.contents
            .insert(object.clone(), StoredObject { version: storage });
    }

    pub(super) fn invalidate(&mut self, id: SnapshotId, inputs: &Rc<BTreeSet<ValuesPath>>) {
        let storage = |multiplicity| {
            Rc::new(ObjectVersion {
                revision: id,
                multiplicity,
                data: ObjectData::Widened {
                    inputs: Rc::clone(inputs),
                },
            })
        };
        // Discarded contents must release the frozen roots they capture.
        self.contents = GenericOrdMap::new();
        let epoch = Rc::new(Epoch {
            inputs: Rc::clone(inputs),
            singleton: storage(Multiplicity::Singleton),
            summary: storage(Multiplicity::Summary),
        });
        self.projection.invalidate(&self.identities, &epoch.inputs);
        self.epoch = Some(epoch);
    }

    pub(super) fn region_projection(&self) -> &Projection {
        &self.projection
    }

    pub(super) fn objects(&self) -> Vec<ObjectId> {
        self.identities.keys().cloned().collect()
    }

    fn shares_epoch(&self, other: &Self) -> bool {
        match (&self.epoch, &other.epoch) {
            (None, None) => true,
            (Some(yes), Some(no)) => Rc::ptr_eq(yes, no),
            (None, Some(_)) | (Some(_), None) => false,
        }
    }

    pub(super) fn join(
        yes: &Self,
        no: &Self,
        mut join: impl FnMut(
            &ObjectId,
            Option<Rc<ObjectVersion>>,
            Option<Rc<ObjectVersion>>,
        ) -> Option<Rc<ObjectVersion>>,
    ) -> Self {
        let mut merged = yes.clone();
        let changed: BTreeSet<_> = if yes.shares_epoch(no) {
            // Both maps skip pointer-equal subtrees before comparing entries.
            let mut changed = BTreeSet::new();
            for difference in yes.contents.diff(&no.contents) {
                let object = match difference {
                    DiffItem::Add(object, _)
                    | DiffItem::Remove(object, _)
                    | DiffItem::Update {
                        old: (object, _),
                        new: _,
                    } => object,
                };
                changed.insert(object);
            }
            for difference in yes.identities.diff(&no.identities) {
                let object = match difference {
                    DiffItem::Add(object, _)
                    | DiffItem::Remove(object, _)
                    | DiffItem::Update {
                        old: (object, _),
                        new: _,
                    } => object,
                };
                changed.insert(object);
            }
            changed
        } else {
            // Distinct epochs change every older entry even when its metadata is shared.
            yes.identities.keys().chain(no.identities.keys()).collect()
        };
        for object in changed {
            tracing::debug_span!(target: "helm_schema_ir::counters::heap_store", "heap_join_diff_objects")
                .in_scope(|| {});
            if let Some(storage) = join(object, yes.object(object), no.object(object)) {
                merged.install(object, storage);
            }
        }
        merged
    }
}

#[cfg(test)]
#[path = "../tests/runtime_heap_store.rs"]
mod tests;
