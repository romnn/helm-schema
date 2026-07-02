use std::collections::HashSet;
use std::sync::Mutex;

/// In-process negative cache shared across K8s and CRD providers.
///
/// Key components:
///   - `source_id`: cache namespace (e.g. `"default"` or a mirror hash).
///   - `bucket`: provider-defined slot (e.g. K8s version dir; for CRDs
///     use the group, or an empty string when not relevant).
///   - `filename`: the resource file the provider tried to fetch.
///
/// Entries persist for the lifetime of the process; we re-attempt
/// across process boundaries because a transient remote 404 should not
/// poison disk-level state.
#[derive(Debug, Default)]
pub struct NegativeCache {
    inner: Mutex<HashSet<(String, String, String)>>,
}

impl NegativeCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a (source_id, bucket, filename) tuple as negative.
    pub fn record(&self, source_id: &str, bucket: &str, filename: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert((
                source_id.to_string(),
                bucket.to_string(),
                filename.to_string(),
            ));
        }
    }

    /// True if this exact tuple has been seen as negative.
    #[must_use]
    pub fn contains(&self, source_id: &str, bucket: &str, filename: &str) -> bool {
        self.inner
            .lock()
            .map(|guard| {
                guard.contains(&(
                    source_id.to_string(),
                    bucket.to_string(),
                    filename.to_string(),
                ))
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
#[path = "tests/negative_cache.rs"]
mod tests;
