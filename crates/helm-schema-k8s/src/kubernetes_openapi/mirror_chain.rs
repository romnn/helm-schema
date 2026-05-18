use crate::cache::{default_source_id, source_id_for_url};

/// A single upstream source (default catalog or user-supplied mirror)
/// used by the K8s OpenAPI provider.
#[derive(Debug, Clone)]
pub struct K8sSource {
    pub source_id: String,
    pub base_url: String,
}

impl K8sSource {
    /// The built-in yannh/kubernetes-json-schema source.
    #[must_use]
    pub fn default_source() -> Self {
        Self {
            source_id: default_source_id().to_string(),
            base_url: "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master"
                .to_string(),
        }
    }

    /// A user-supplied mirror (`--k8s-schema-mirror`). `source_id` is
    /// derived deterministically from the URL.
    #[must_use]
    pub fn mirror(base_url: impl Into<String>) -> Self {
        let url = base_url.into();
        Self {
            source_id: source_id_for_url(&url),
            base_url: url,
        }
    }
}

/// The ordered list of K8s sources to probe. The default catalog
/// always comes first; user-supplied mirrors append in user-supplied
/// order.
#[derive(Debug, Clone)]
pub struct K8sMirrorChain {
    pub sources: Vec<K8sSource>,
}

impl K8sMirrorChain {
    /// Build a chain with `default` first and any mirrors appended.
    #[must_use]
    pub fn with_mirrors(mirrors: Vec<String>) -> Self {
        let mut sources = vec![K8sSource::default_source()];
        for url in mirrors {
            sources.push(K8sSource::mirror(url));
        }
        Self { sources }
    }
}

impl Default for K8sMirrorChain {
    fn default() -> Self {
        Self::with_mirrors(Vec::new())
    }
}
