use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Mutex, MutexGuard};

/// Check-compute-insert memo backing the chain's per-run caches.
///
/// The guard is not held while `compute` runs, so a re-entrant lookup
/// cannot deadlock; a racing duplicate compute just overwrites the
/// entry with an equal value. Poisoned locks are recovered — a
/// panicking writer can only have completed or skipped an insert, so
/// the map itself stays coherent.
#[derive(Debug)]
pub(crate) struct MemoCache<K, V> {
    entries: Mutex<HashMap<K, V>>,
}

// Manual impl: `derive(Default)` would demand `K: Default, V: Default`
// even though an empty map needs neither.
impl<K, V> Default for MemoCache<K, V> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<K, V> MemoCache<K, V>
where
    K: Eq + Hash,
    V: Clone,
{
    pub(crate) fn get_or_compute(&self, key: K, compute: impl FnOnce() -> V) -> V {
        if let Some(cached) = self.entries().get(&key) {
            return cached.clone();
        }
        let value = compute();
        self.entries().insert(key, value.clone());
        value
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<K, V>> {
        match self.entries.lock() {
            Ok(guard) => guard,
            Err(err) => err.into_inner(),
        }
    }
}
