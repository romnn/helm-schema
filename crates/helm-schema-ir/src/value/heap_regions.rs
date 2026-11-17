//! Unknown-region observations of immutable store roots and their input registrations.

#[cfg(test)]
#[path = "../tests/runtime_heap_regions.rs"]
mod tests;

use std::collections::BTreeSet;
use std::rc::Rc;

use ahash::{AHashMap, AHashSet};
use helm_schema_core::ValuesPath;

use super::{HeapVersion, ObjectData, Remainder};

#[tracing::instrument(skip_all, fields(root_count = roots.len()))]
pub(super) fn paths(
    inputs: &super::input_paths::InputPaths,
    roots: &[&HeapVersion],
) -> BTreeSet<ValuesPath> {
    let mut paths = BTreeSet::new();
    let mut input_sets = AHashSet::new();
    let mut pending_inputs = AHashMap::new();
    inputs.visit(|object, path| {
        pending_inputs.insert(object.clone(), path);
    });
    let mut previous = None;
    for root in roots {
        if let Some(known) = root.store.fallback_inputs() {
            if input_sets.insert(Rc::as_ptr(known)) {
                paths.extend(known.iter().cloned());
            }
            // Only unoverridden, allocated identities use the epoch's original-input coverage.
            if !pending_inputs.is_empty() {
                pending_inputs.retain(|object, path| {
                    if root.store.is_fallback(object) {
                        paths.insert((**path).clone());
                        false
                    } else {
                        true
                    }
                });
            }
        }
        let visit = |object, storage: &Rc<super::ObjectVersion>| {
            if append_data_regions(&storage.data, &mut input_sets, &mut paths)
                && !pending_inputs.is_empty()
                && let Some(path) = pending_inputs.remove(&object)
            {
                paths.insert((*path).clone());
            }
        };
        if let Some(previous) = previous {
            root.store.visit_explicit_since(previous, visit);
        } else {
            root.store.visit_explicit(visit);
        }
        previous = Some(&root.store);
    }
    paths
}

fn append_data_regions(
    data: &ObjectData,
    input_sets: &mut AHashSet<*const BTreeSet<ValuesPath>>,
    paths: &mut BTreeSet<ValuesPath>,
) -> bool {
    let known = match data {
        ObjectData::Widened { inputs }
        | ObjectData::Map {
            fields: _,
            remainder: Remainder::Unknown { inputs },
        } => Some(inputs),
        ObjectData::Map {
            fields: _,
            remainder: Remainder::Open | Remainder::Layered(_),
        } => None,
        ObjectData::Map {
            fields: _,
            remainder: Remainder::Closed | Remainder::Input,
        }
        | ObjectData::Strings(_)
        | ObjectData::Opaque { reachable: _ } => return false,
    };
    if let Some(known) = known
        && input_sets.insert(Rc::as_ptr(known))
    {
        paths.extend(known.iter().cloned());
    }
    true
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
