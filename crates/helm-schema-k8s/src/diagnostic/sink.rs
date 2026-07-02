use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::diagnostic::{Diagnostic, DiagnosticKey};

/// Thread-safe sink for [`Diagnostic`] events keyed by
/// [`DiagnosticKey`]. Uses `BTreeMap` so iteration order is
/// deterministic (driven by `DiagnosticKey`'s `Ord`) independent of
/// insertion order. First writer per key wins; canonicalisation at
/// insertion time means payloads for the same key are identical by
/// construction.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticSink {
    inner: Arc<Mutex<BTreeMap<DiagnosticKey, Diagnostic>>>,
}

impl DiagnosticSink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a diagnostic if no diagnostic with the same key has been
    /// inserted yet. Canonicalises the payload before insertion so
    /// later equality checks are stable.
    pub fn push(&self, mut diagnostic: Diagnostic) {
        diagnostic.canonicalise();
        if let Ok(mut guard) = self.inner.lock() {
            guard.entry(diagnostic.key()).or_insert(diagnostic);
        }
    }

    /// Run a closure over the BTreeMap iterator. Held under the mutex
    /// for the duration of the closure; keep it short.
    pub fn for_each<F: FnMut(&Diagnostic)>(&self, mut f: F) {
        if let Ok(guard) = self.inner.lock() {
            for diagnostic in guard.values() {
                f(diagnostic);
            }
        }
    }

    /// Snapshot the diagnostics as a new `Vec`. Useful for tests; in
    /// hot paths prefer `for_each`.
    #[must_use]
    pub fn snapshot(&self) -> Vec<Diagnostic> {
        self.inner
            .lock()
            .map(|g| g.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Number of distinct diagnostic keys currently held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// True when no diagnostics have been emitted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
