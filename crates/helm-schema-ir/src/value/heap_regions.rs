//! Unknown-region observations of immutable store roots and their input registrations.

#[cfg(test)]
#[path = "../tests/runtime_heap_regions.rs"]
mod tests;

use std::collections::BTreeSet;
use std::rc::Rc;

use ahash::AHashSet;
use helm_schema_core::ValuesPath;
use imbl::GenericOrdMap;
use imbl::ordmap::DiffItem;
use imbl::shared_ptr::RcK;

use super::{HeapVersion, Multiplicity, ObjectData, ObjectId, Remainder};

type OriginalObjects = GenericOrdMap<ObjectId, Multiplicity, RcK>;
type KnownObjects = GenericOrdMap<ObjectId, KnownInputs, RcK>;

#[derive(Clone, Debug)]
struct KnownInputs {
    inputs: Rc<BTreeSet<ValuesPath>>,
}

impl PartialEq for KnownInputs {
    fn eq(&self, other: &Self) -> bool {
        let Self { inputs } = self;
        let Self {
            inputs: other_inputs,
        } = other;
        Rc::ptr_eq(inputs, other_inputs)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Projection {
    // Only keys matter; the identity-map type makes an epoch reset a shared clone.
    originals: Option<Rc<OriginalObjects>>,
    known: Option<Rc<KnownObjects>>,
    epoch_inputs: Option<Rc<BTreeSet<ValuesPath>>>,
    // Explicit objects with an equal input set support the epoch contribution too.
    epoch_objects: usize,
}

#[derive(Default)]
pub(super) struct ObjectRegions {
    original: bool,
    inputs: Option<Rc<BTreeSet<ValuesPath>>>,
}

impl From<&ObjectData> for ObjectRegions {
    fn from(data: &ObjectData) -> Self {
        match data {
            ObjectData::Widened { inputs }
            | ObjectData::Map {
                fields: _,
                remainder: Remainder::Unknown { inputs },
            } => Self {
                original: true,
                inputs: (!inputs.is_empty()).then(|| Rc::clone(inputs)),
            },
            ObjectData::Map {
                fields: _,
                remainder: Remainder::Open | Remainder::Layered(_),
            } => Self {
                original: true,
                inputs: None,
            },
            ObjectData::Map {
                fields: _,
                remainder: Remainder::Closed | Remainder::Input,
            }
            | ObjectData::Strings(_)
            | ObjectData::Opaque { reachable: _ } => Self::default(),
        }
    }
}

fn same_inputs(
    yes: Option<&Rc<BTreeSet<ValuesPath>>>,
    no: Option<&Rc<BTreeSet<ValuesPath>>>,
) -> bool {
    match (yes, no) {
        (None, None) => true,
        (Some(yes), Some(no)) => Rc::ptr_eq(yes, no) || yes == no,
        (None, Some(_)) | (Some(_), None) => false,
    }
}

impl Projection {
    pub(super) fn install(
        &mut self,
        object: &ObjectId,
        multiplicity: Multiplicity,
        before: ObjectRegions,
        after: ObjectRegions,
    ) {
        let ObjectRegions {
            original: before_original,
            inputs: before_inputs,
        } = before;
        let ObjectRegions {
            original: after_original,
            inputs: after_inputs,
        } = after;
        if before_original != after_original {
            if after_original {
                let originals = self
                    .originals
                    .get_or_insert_with(|| Rc::new(OriginalObjects::new()));
                Rc::make_mut(originals).insert(object.clone(), multiplicity);
            } else if let Some(originals) = &mut self.originals {
                Rc::make_mut(originals).remove(object);
                if originals.is_empty() {
                    self.originals = None;
                }
            }
        }
        if same_inputs(before_inputs.as_ref(), after_inputs.as_ref()) {
            return;
        }
        if before_inputs.is_some()
            && same_inputs(before_inputs.as_ref(), self.epoch_inputs.as_ref())
        {
            self.epoch_objects -= 1;
        }
        let after_epoch = after_inputs.is_some()
            && same_inputs(after_inputs.as_ref(), self.epoch_inputs.as_ref());
        if after_epoch {
            self.epoch_objects += 1;
        }
        if let Some(inputs) = after_inputs.filter(|_| !after_epoch) {
            let known = self
                .known
                .get_or_insert_with(|| Rc::new(KnownObjects::new()));
            Rc::make_mut(known).insert(object.clone(), KnownInputs { inputs });
        } else if let Some(known) = &mut self.known
            && known.contains_key(object)
        {
            Rc::make_mut(known).remove(object);
            if known.is_empty() {
                self.known = None;
            }
        }
    }

    pub(super) fn invalidate(
        &mut self,
        identities: &OriginalObjects,
        inputs: &Rc<BTreeSet<ValuesPath>>,
    ) {
        if identities.is_empty() {
            self.originals = None;
        } else if self
            .originals
            .as_ref()
            .is_none_or(|originals| !originals.ptr_eq(identities))
        {
            self.originals = Some(Rc::new(identities.clone()));
        }
        self.known = None;
        let inputs = (!inputs.is_empty()).then(|| Rc::clone(inputs));
        self.epoch_objects = if inputs.is_some() {
            identities.len()
        } else {
            0
        };
        if !same_inputs(self.epoch_inputs.as_ref(), inputs.as_ref()) {
            self.epoch_inputs = inputs;
        }
    }
}

fn union_keys<K: Clone + Ord, V: Clone + PartialEq>(
    left: &mut GenericOrdMap<K, V, RcK>,
    right: &GenericOrdMap<K, V, RcK>,
) {
    if right.is_empty() || left.ptr_eq(right) {
        return;
    }
    if left.is_empty() {
        *left = right.clone();
        return;
    }
    // Small inputs avoid scanning removals; comparable maps skip their shared subtrees.
    if right.len() < left.len() / 2 {
        for (key, value) in right {
            if !left.contains_key(key) {
                left.insert(key.clone(), value.clone());
            }
        }
        return;
    }
    let before = left.clone();
    for difference in right.diff(&before) {
        let key = match difference {
            DiffItem::Add(key, _value) | DiffItem::Remove(key, _value) => key,
            DiffItem::Update {
                old: (key, _old_value),
                new: (_new_key, _new_value),
            } => key,
        };
        if !left.contains_key(key)
            && let Some(value) = right.get(key)
        {
            left.insert(key.clone(), value.clone());
        }
    }
}

#[tracing::instrument(skip_all, fields(root_count = roots.len()))]
pub(super) fn paths(
    inputs: &super::input_paths::InputPaths,
    roots: &[&HeapVersion],
) -> BTreeSet<ValuesPath> {
    let mut paths = BTreeSet::new();
    let mut input_sets = AHashSet::new();
    let mut originals = OriginalObjects::new();
    let mut previous: Option<&KnownObjects> = None;
    for root in roots {
        let projection = root.store.region_projection();
        if let Some(selected) = &projection.originals {
            union_keys(&mut originals, selected);
        }
        let mut append = |known: &Rc<BTreeSet<ValuesPath>>| {
            if input_sets.insert(Rc::as_ptr(known)) {
                paths.extend(known.iter().cloned());
            }
        };
        if projection.epoch_objects > 0
            && let Some(known) = &projection.epoch_inputs
        {
            append(known);
        }
        if let Some(known) = projection.known.as_deref() {
            if let Some(previous) = previous {
                for difference in known.diff(previous) {
                    let object = match difference {
                        DiffItem::Add(object, _inputs) | DiffItem::Remove(object, _inputs) => {
                            object
                        }
                        DiffItem::Update {
                            old: (object, _old_inputs),
                            new: (_new_object, _new_inputs),
                        } => object,
                    };
                    if let Some(KnownInputs { inputs }) = known.get(object) {
                        append(inputs);
                    }
                }
            } else {
                for KnownInputs { inputs } in known.values() {
                    append(inputs);
                }
            }
        }
        previous = projection.known.as_deref();
    }
    if !originals.is_empty() {
        inputs.visit(|object, path| {
            if originals.contains_key(object) {
                paths.insert(path.clone());
            }
        });
    }
    paths
}

pub(super) fn project(
    condition: &helm_schema_core::Predicate,
    inputs: &super::input_paths::InputPaths,
    roots: &[&HeapVersion],
    reads: &mut super::read_set::ReadSet,
) {
    for path in paths(inputs, roots) {
        reads.insert(super::RuntimeRead {
            path,
            condition: condition.clone(),
            coverage: super::ReadCoverage::UnknownRegion,
        });
    }
}
