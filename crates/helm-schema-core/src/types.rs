use serde::{Deserialize, Serialize};

use crate::{HelperBranch, Predicate};

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

/// One arm of a values-predicate-selected `kind:` chain
/// (`kind: {{ if $stateful }}StatefulSet{{ else }}Deployment{{ end }}`).
///
/// The predicate holds exactly where this arm's kind is the document's
/// kind. It is lowered in the same template scope as the body's own branch
/// conditions, so a use conjunction that carries the selecting predicate
/// entails the arm structurally.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KindBranch {
    pub predicate: Predicate,
    pub kind: String,
}

/// Detected Kubernetes resource type (apiVersion + kind).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub api_version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kind_candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_branches: Vec<HelperBranch>,
    /// Predicate-qualified alternatives behind an inline-conditional
    /// `kind:`. An IR-internal enrichment: attached at use-tagging time
    /// (the selecting locals resolve only in template scope) and consumed
    /// by the contract-signal builder's per-row kind concretization, so it
    /// never serializes.
    #[serde(skip)]
    pub kind_branches: Vec<KindBranch>,
}

impl ResourceRef {
    /// Resource with one exact apiVersion and no alternative candidates or
    /// branch-aware apiVersion output.
    #[must_use]
    pub fn concrete(api_version: String, kind: String) -> Self {
        Self {
            api_version,
            kind,
            kind_candidates: Vec::new(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
            kind_branches: Vec::new(),
        }
    }
}
