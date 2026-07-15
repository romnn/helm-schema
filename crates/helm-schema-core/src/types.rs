use serde::{Deserialize, Serialize};

use crate::HelperBranch;

/// YAML path in the rendered manifest, e.g. `["metadata", "name"]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct YamlPath(pub Vec<String>);

/// How a value contributes to rendered YAML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ValueKind {
    Scalar = 0,
    PartialScalar = 1,
    Fragment = 2,
    /// A serialization transform preserves dependency provenance without
    /// exposing input shape.
    Serialized = 3,
    /// `toYaml` accepts any input shape, while the rendered YAML fragment's
    /// structural placement can still constrain the resulting document.
    YamlSerialized = 4,
}

/// Detected Kubernetes resource type (apiVersion + kind).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub api_version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_branches: Vec<HelperBranch>,
}

impl ResourceRef {
    /// Resource with one exact apiVersion and no alternative candidates or
    /// branch-aware apiVersion output.
    #[must_use]
    pub fn concrete(api_version: String, kind: String) -> Self {
        Self {
            api_version,
            kind,
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }
    }
}
