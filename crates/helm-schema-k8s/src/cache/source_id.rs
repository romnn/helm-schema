use sha2::{Digest, Sha256};

/// Stable per-source identifier used to namespace cache entries.
///
/// `default` is reserved for the built-in upstream URL of each provider
/// (yannh/kubernetes-json-schema for K8s, datreeio/CRDs-catalog for
/// CRDs). Any user-supplied mirror URL gets the first 12 hex chars of
/// `SHA-256(url)` so a given URL always maps to the same `source_id`
/// across runs and across providers.
#[must_use]
pub const fn default_source_id() -> &'static str {
    "default"
}

/// Hash a mirror URL into a stable 12-hex-char short identifier.
#[must_use]
pub fn source_id_for_url(url: &str) -> String {
    let digest = Sha256::digest(url.as_bytes());
    let hex = format!("{digest:x}");
    hex.chars().take(12).collect()
}

#[cfg(test)]
#[path = "tests/source_id.rs"]
mod tests;
