use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug)]
enum NegativeEvidence {
    Absent,
    Superseded,
}

/// In-process negative cache shared across K8s and CRD providers.
///
/// Key components:
///   - `source_id`: cache namespace (e.g. `"default"` or a mirror hash).
///   - `bucket`: provider-defined slot (e.g. K8s version dir; for CRDs
///     use the group, or an empty string when not relevant).
///   - `filename`: the resource file the provider tried to fetch.
///
/// Entries record clean upstream not-found responses until a successful response
/// supersedes them.
/// Superseded entries prevent surviving disk markers from restoring stale absence.
/// Kubernetes also persists disk not-found markers; CRD catalog negatives stay in memory.
#[derive(Debug, Default)]
pub struct NegativeCache {
    inner: Mutex<HashMap<(String, String, String), NegativeEvidence>>,
}

impl NegativeCache {
    /// Creates an empty in-process negative cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a (`source_id`, bucket, filename) tuple as negative.
    pub fn record(&self, source_id: &str, bucket: &str, filename: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(
                (
                    source_id.to_string(),
                    bucket.to_string(),
                    filename.to_string(),
                ),
                NegativeEvidence::Absent,
            );
        }
    }

    pub(crate) fn supersede(&self, source_id: &str, bucket: &str, filename: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(
                (
                    source_id.to_string(),
                    bucket.to_string(),
                    filename.to_string(),
                ),
                NegativeEvidence::Superseded,
            );
        }
    }

    /// True if this exact tuple currently records an upstream not-found response.
    #[must_use]
    pub fn contains(&self, source_id: &str, bucket: &str, filename: &str) -> bool {
        self.contains_or_marker(source_id, bucket, filename, false)
    }

    pub(crate) fn contains_or_marker(
        &self,
        source_id: &str,
        bucket: &str,
        filename: &str,
        marker_exists: bool,
    ) -> bool {
        self.inner
            .lock()
            .map(|guard| {
                match guard.get(&(
                    source_id.to_string(),
                    bucket.to_string(),
                    filename.to_string(),
                )) {
                    Some(NegativeEvidence::Absent) => true,
                    Some(NegativeEvidence::Superseded) => false,
                    None => marker_exists,
                }
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
#[path = "tests/negative_cache.rs"]
mod tests;
