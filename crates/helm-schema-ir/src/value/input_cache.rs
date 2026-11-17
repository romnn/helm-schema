//! Evictable input-provenance results of frozen values.

#[cfg(test)]
#[path = "../tests/runtime_input_cache.rs"]
mod tests;

use super::ValueNode;
use ahash::AHashMap;
use helm_schema_core::ValuesPath;
use std::collections::{BTreeSet, VecDeque};
use std::rc::{Rc, Weak};

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
    pub(super) work: usize,
    weight: usize,
}

#[derive(Debug)]
pub(super) struct InputCache {
    entries: AHashMap<InputQueryKey, InputEntry>,
    order: VecDeque<InputQueryKey>,
    payload: usize,
    max_entries: usize,
    max_payload: usize,
    stats: InputStats,
}

impl Default for InputCache {
    fn default() -> Self {
        Self {
            entries: AHashMap::new(),
            order: VecDeque::new(),
            payload: 0,
            max_entries: 4096,
            max_payload: 262_144,
            stats: InputStats::default(),
        }
    }
}

#[derive(Debug, Default)]
struct InputStats {
    hits: u64,
    misses: u64,
}

impl Drop for InputStats {
    #[tracing::instrument(skip_all, target = "helm_schema_ir::value::input_cache::summary", fields(hits = self.hits, misses = self.misses))]
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
        work: usize,
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
        if self.max_entries == 0 || weight > self.max_payload || self.entries.contains_key(&key) {
            return;
        }
        while self.entries.len() >= self.max_entries || self.payload + weight > self.max_payload {
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
                work,
                weight,
            },
        );
        self.payload += weight;
        self.order.push_back(key);
    }
}
