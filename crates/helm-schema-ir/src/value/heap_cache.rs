//! Bounded derived object views, accounting shared provenance only once.

use std::collections::{BTreeSet, VecDeque};
use std::rc::{Rc, Weak};

use ahash::AHashMap;
use helm_schema_core::ValuesPath;

use super::heap_query::ObjectQueryKey;
use super::{AliasSet, Multiplicity, ObjectData, ObjectVersion, Remainder, SnapshotId, ValueNode};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct InputQueryKey {
    pub(super) version: u64,
    pub(super) value: *const ValueNode,
    pub(super) include_unknown_aliases: bool,
}

#[derive(Debug)]
pub(super) struct InputEntry {
    value: Weak<ValueNode>,
    pub(super) inputs: BTreeSet<ValuesPath>,
    weight: usize,
}

#[derive(Debug, Default)]
pub(super) struct InputCache {
    entries: AHashMap<InputQueryKey, InputEntry>,
    order: VecDeque<InputQueryKey>,
    payload: usize,
    stats: InputStats,
}

#[derive(Debug, Default)]
struct InputStats {
    hits: u64,
    misses: u64,
}

impl Drop for InputStats {
    #[tracing::instrument(skip_all, target = "helm_schema_ir::value::heap_cache::summary", fields(hits = self.hits, misses = self.misses))]
    fn drop(&mut self) {}
}

impl InputCache {
    pub(super) fn get(&mut self, key: &InputQueryKey) -> Option<&InputEntry> {
        let Some(entry) = self.entries.get(key) else {
            self.stats.misses += 1;
            return None;
        };
        self.stats.hits += 1;
        std::ptr::eq(entry.value.as_ptr(), key.value).then_some(entry)
    }

    pub(super) fn insert(
        &mut self,
        key: InputQueryKey,
        value: &Rc<ValueNode>,
        inputs: &BTreeSet<ValuesPath>,
    ) {
        let weight = inputs.iter().fold(1_usize, |weight, path| {
            path.segments()
                .fold(weight.saturating_add(1), |weight, segment| {
                    weight.saturating_add(
                        segment
                            .literal()
                            .map_or(1, |key| key.len().saturating_add(1)),
                    )
                })
        });
        if weight > 262_144 || self.entries.contains_key(&key) {
            return;
        }
        while self.entries.len() >= 4096 || self.payload + weight > 262_144 {
            let Some(oldest) = self.order.pop_front() else {
                return;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.payload -= entry.weight;
            }
        }
        self.entries.insert(
            key,
            InputEntry {
                value: Rc::downgrade(value),
                inputs: inputs.clone(),
                weight,
            },
        );
        self.payload += weight;
        self.order.push_back(key);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct WidenedKey {
    version: u64,
    summary: bool,
    inputs: *const BTreeSet<ValuesPath>,
}

#[derive(Debug, Default)]
pub(super) struct WidenedCache {
    entries: AHashMap<WidenedKey, Weak<ObjectVersion>>,
    order: VecDeque<WidenedKey>,
}

impl WidenedCache {
    pub(super) fn object(
        &mut self,
        revision: SnapshotId,
        multiplicity: Multiplicity,
        inputs: &Rc<BTreeSet<ValuesPath>>,
    ) -> Rc<ObjectVersion> {
        let SnapshotId { ordinal: version } = revision;
        let key = WidenedKey {
            version,
            summary: match multiplicity {
                Multiplicity::Singleton => false,
                Multiplicity::Summary => true,
            },
            inputs: Rc::as_ptr(inputs),
        };
        if let Some(answer) = self.entries.get(&key).and_then(Weak::upgrade) {
            return answer;
        }
        let answer = Rc::new(ObjectVersion {
            revision,
            multiplicity,
            data: ObjectData::Widened {
                inputs: Rc::clone(inputs),
            },
        });
        if !self.entries.contains_key(&key) {
            while self.entries.len() >= 32_768 {
                if let Some(oldest) = self.order.pop_front() {
                    self.entries.remove(&oldest);
                } else {
                    return answer;
                }
            }
            self.order.push_back(key);
        }
        self.entries.insert(key, Rc::downgrade(&answer));
        answer
    }
}

const MAX_ENTRIES: usize = 2_097_152;
const MAX_PAYLOAD: usize = 4_194_304;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ObjectLookup {
    Miss,
    Hit(Option<Rc<ObjectVersion>>),
}

#[derive(Debug)]
struct CacheLimits {
    entries: usize,
    payload: usize,
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            entries: MAX_ENTRIES,
            payload: MAX_PAYLOAD,
        }
    }
}

#[cfg(test)]
#[path = "../tests/runtime_heap_cache.rs"]
mod tests;

#[derive(Debug)]
struct Entry {
    answer: Option<Rc<ObjectVersion>>,
    referenced: bool,
    initial_position: u32,
}

#[derive(Debug)]
struct RetainedInputs {
    references: usize,
    weight: usize,
}

#[derive(Debug, Default)]
pub(super) struct ObjectCache {
    limits: CacheLimits,
    entries: AHashMap<ObjectQueryKey, Entry>,
    order: Option<VecDeque<ObjectQueryKey>>,
    inputs: AHashMap<*const BTreeSet<ValuesPath>, RetainedInputs>,
    payload: usize,
    stats: CacheStats,
}

#[derive(Debug, Default)]
struct CacheStats {
    hits: u64,
    misses: u64,
    insertions: u64,
    evictions: u64,
    declined: u64,
}

impl Drop for CacheStats {
    #[tracing::instrument(skip_all, target = "helm_schema_ir::value::heap_cache::summary", fields(hits = self.hits, misses = self.misses, insertions = self.insertions, evictions = self.evictions, declined = self.declined, object_bytes = std::mem::size_of::<ObjectVersion>(), alias_bytes = std::mem::size_of::<AliasSet>()))]
    fn drop(&mut self) {}
}

impl ObjectCache {
    fn ensure_eviction_order(&mut self) {
        if self.order.is_some() {
            return;
        }
        let mut keys = self
            .entries
            .iter()
            .map(|(key, entry)| (entry.initial_position, *key))
            .collect::<Vec<_>>();
        keys.sort_unstable_by_key(|(position, _)| *position);
        self.order = Some(keys.into_iter().map(|(_, key)| key).collect());
    }

    pub(super) fn get(&mut self, key: &ObjectQueryKey) -> ObjectLookup {
        let Some(entry) = self.entries.get_mut(key) else {
            self.stats.misses += 1;
            return ObjectLookup::Miss;
        };
        self.stats.hits += 1;
        entry.referenced = true;
        ObjectLookup::Hit(entry.answer.clone())
    }

    pub(super) fn insert(&mut self, key: ObjectQueryKey, answer: Option<&Rc<ObjectVersion>>) {
        let weight = storage_weight(answer);
        let shared = shared_inputs(answer);
        if weight + shared.map_or(0, |inputs| inputs.len() + 1) > self.limits.payload {
            if !self.entries.contains_key(&key) {
                self.stats.declined += 1;
            }
            return;
        }
        if self.entries.len() >= self.limits.entries
            || self.payload + weight + self.additional_inputs(shared) > self.limits.payload
        {
            if self.entries.contains_key(&key) {
                return;
            }
            while self.entries.len() >= self.limits.entries
                || self.payload + weight + self.additional_inputs(shared) > self.limits.payload
            {
                self.ensure_eviction_order();
                let Some(oldest) = self.order.as_mut().and_then(VecDeque::pop_front) else {
                    return;
                };
                if let Some(entry) = self.entries.get_mut(&oldest)
                    && entry.referenced
                {
                    entry.referenced = false;
                    if let Some(order) = &mut self.order {
                        order.push_back(oldest);
                    }
                    continue;
                }
                if let Some(entry) = self.entries.remove(&oldest) {
                    self.stats.evictions += 1;
                    self.payload -= storage_weight(entry.answer.as_ref());
                    if let Some(inputs) = shared_inputs(entry.answer.as_ref()) {
                        let key = Rc::as_ptr(inputs);
                        if let Some(retained) = self.inputs.get_mut(&key) {
                            retained.references -= 1;
                            if retained.references == 0 {
                                self.payload -= retained.weight;
                                self.inputs.remove(&key);
                            }
                        }
                    }
                }
            }
        }
        let initial_position = u32::try_from(self.entries.len()).ok();
        if initial_position.is_none() {
            self.ensure_eviction_order();
        }
        let std::collections::hash_map::Entry::Vacant(slot) = self.entries.entry(key) else {
            return;
        };
        self.payload += weight;
        self.stats.insertions += 1;
        if let Some(inputs) = shared {
            let retained = self.inputs.entry(Rc::as_ptr(inputs)).or_insert_with(|| {
                let weight = inputs.len() + 1;
                self.payload += weight;
                RetainedInputs {
                    references: 0,
                    weight,
                }
            });
            retained.references += 1;
        }
        slot.insert(Entry {
            answer: answer.cloned(),
            referenced: false,
            initial_position: initial_position.unwrap_or_default(),
        });
        if let Some(order) = &mut self.order {
            order.push_back(key);
        }
    }

    fn additional_inputs(&self, inputs: Option<&Rc<BTreeSet<ValuesPath>>>) -> usize {
        inputs
            .filter(|inputs| !self.inputs.contains_key(&Rc::as_ptr(inputs)))
            .map_or(0, |inputs| inputs.len() + 1)
    }
}

fn shared_inputs(answer: Option<&Rc<ObjectVersion>>) -> Option<&Rc<BTreeSet<ValuesPath>>> {
    let ObjectVersion {
        revision: _,
        multiplicity: _,
        data,
    } = answer?.as_ref();
    match data {
        ObjectData::Widened { inputs }
        | ObjectData::Map {
            fields: _,
            remainder: Remainder::Unknown { inputs },
        } => Some(inputs),
        ObjectData::Map {
            fields: _,
            remainder: Remainder::Closed | Remainder::Open | Remainder::Input,
        }
        | ObjectData::Strings(_)
        | ObjectData::Opaque { reachable: _ } => None,
    }
}

fn storage_weight(answer: Option<&Rc<ObjectVersion>>) -> usize {
    let Some(answer) = answer else {
        return 1;
    };
    let ObjectVersion {
        revision: _,
        multiplicity: _,
        data,
    } = answer.as_ref();
    match data {
        ObjectData::Map {
            fields,
            remainder: _,
        } => fields.len() + 1,
        ObjectData::Strings(slots) => slots.len() + 1,
        ObjectData::Opaque {
            reachable: AliasSet::Known(objects),
        } => objects.len() + 1,
        ObjectData::Opaque {
            reachable: AliasSet::RenderGraph,
        }
        | ObjectData::Widened { inputs: _ } => 1,
    }
}
