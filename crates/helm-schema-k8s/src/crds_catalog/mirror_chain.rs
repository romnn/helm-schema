use crate::cache::{default_source_id, source_id_for_url};

/// A single upstream CRD catalog source.
#[derive(Debug, Clone)]
pub struct CrdSource {
    pub source_id: String,
    pub base_url: String,
}

impl CrdSource {
    /// The built-in datreeio/CRDs-catalog source.
    #[must_use]
    pub fn default_source() -> Self {
        Self {
            source_id: default_source_id().to_string(),
            base_url: "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main".to_string(),
        }
    }

    /// A user-supplied mirror (`--crd-catalog-mirror`). `source_id` is
    /// derived from the URL.
    #[must_use]
    pub fn mirror(base_url: impl Into<String>) -> Self {
        let url = base_url.into();
        Self {
            source_id: source_id_for_url(&url),
            base_url: url,
        }
    }
}

/// The ordered list of CRD sources to probe. Default first; mirrors
/// in user-supplied order.
#[derive(Debug, Clone)]
pub struct CrdMirrorChain {
    pub sources: Vec<CrdSource>,
}

impl CrdMirrorChain {
    #[must_use]
    pub fn with_mirrors(mirrors: Vec<String>) -> Self {
        let mut sources = vec![CrdSource::default_source()];
        for url in mirrors {
            sources.push(CrdSource::mirror(url));
        }
        Self { sources }
    }
}

impl Default for CrdMirrorChain {
    fn default() -> Self {
        Self::with_mirrors(Vec::new())
    }
}
