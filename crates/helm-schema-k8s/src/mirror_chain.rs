use std::collections::HashSet;

use crate::cache::{default_source_id, source_id_for_url};

/// A single upstream schema source (default catalog or user-supplied mirror).
#[derive(Debug, Clone)]
pub(crate) struct SchemaSource {
    pub source_id: String,
    pub base_url: String,
}

impl SchemaSource {
    /// The built-in default catalog source for a provider.
    #[must_use]
    pub(crate) fn default_source(base_url: &str) -> Self {
        Self {
            source_id: default_source_id().to_string(),
            base_url: base_url.to_string(),
        }
    }

    /// A user-supplied mirror. `source_id` is derived deterministically
    /// from the URL.
    #[must_use]
    pub fn mirror(base_url: impl Into<String>) -> Self {
        let url = base_url.into();
        Self {
            source_id: source_id_for_url(&url),
            base_url: url,
        }
    }
}

/// The ordered list of sources to probe. The default catalog always comes
/// first; user-supplied mirrors append in user-supplied order.
#[derive(Debug, Clone)]
pub(crate) struct MirrorChain {
    pub sources: Vec<SchemaSource>,
}

impl MirrorChain {
    /// Build a chain with the default source first and any mirrors appended.
    #[must_use]
    pub fn with_mirrors(default_base_url: &str, mirrors: Vec<String>) -> Self {
        let mut sources = vec![SchemaSource::default_source(default_base_url)];
        for url in mirrors {
            sources.push(SchemaSource::mirror(url));
        }
        Self { sources }
    }

    /// The set of source-id directory names currently configured (`default`
    /// plus any user-supplied mirrors). Cache scans MUST consult only these;
    /// on-disk dirs from previously-removed mirrors are stale and must not
    /// influence live inference or cross-version hints.
    #[must_use]
    pub(crate) fn source_ids(&self) -> HashSet<String> {
        self.sources
            .iter()
            .map(|source| source.source_id.clone())
            .collect()
    }
}
