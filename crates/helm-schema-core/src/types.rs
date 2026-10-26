use serde::{Deserialize, Serialize};

use crate::{HelperBranch, Predicate, ValuesPath};

/// YAML path in the rendered manifest, e.g. `["metadata", "name"]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct YamlPath(pub Vec<String>);

/// Positive placement knowledge from a parsed executed-output scaffold.
///
/// This certifies context for bounded preimages, not the input's value or rendered bytes.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum PlainScalarPlacement {
    /// The output context has no positive block-value proof.
    #[default]
    Unknown,
    /// One unknown emission fills a plain block-map or block-sequence value without flow ancestors.
    ///
    /// Arbitrary inputs may still change the document. This permits only explicitly justified
    /// context-sensitive preimages, rather than a general claim that layout survives substitution.
    BlockValue,
}

/// How a value contributes to rendered YAML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ValueKind {
    /// A complete scalar value.
    Scalar = 0,
    /// Text contributing only part of a scalar token.
    PartialScalar = 1,
    /// A structurally parsed YAML fragment.
    Fragment = 2,
    /// A serialization transform preserves dependency provenance without
    /// exposing input shape.
    Serialized = 3,
    /// `toYaml` accepts any input shape, while the rendered YAML fragment's
    /// structural placement can still constrain the resulting document.
    YamlSerialized = 4,
    /// `tpl (toYaml …)` preserves the serialized YAML structure, while
    /// template actions may replace constrained string leaves before the
    /// provider consumes them.
    TemplatedYamlSerialized = 5,
    /// A resource-bound helper analysis retained only that this values path
    /// was part of the eagerly evaluated input.
    ///
    /// This admits the path beneath closed roots without claiming a rendered
    /// shape, serialization transform, or non-control use.
    WidenedDependency = 6,
    /// Execution may read unnamed inputs below this region.
    ///
    /// This withdraws inferred member closure without claiming a container kind.
    WidenedRegion = 7,
    /// A raw map contributes Go-formatted text in a complete YAML scalar slot.
    /// Only bounded safe-content subsets can prove its resulting YAML type.
    GoFormattedMap = 8,
    /// A raw sequence contributes Go-formatted text in a complete YAML scalar slot.
    /// Its source items never identify the reparsed YAML items.
    GoFormattedSequence = 9,
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
    /// Predicate selecting this kind alternative.
    pub predicate: Predicate,
    /// Kubernetes kind emitted while the predicate holds.
    pub kind: String,
}

/// Detected Kubernetes resource type (apiVersion + kind).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    /// Primary API version written or inferred for the resource.
    pub api_version: String,
    /// Primary Kubernetes resource kind.
    pub kind: String,
    /// Additional statically reachable kind literals.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kind_candidates: Vec<String>,
    /// Additional statically reachable API-version literals.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_candidates: Vec<String>,
    /// Guarded helper branches that produce API-version alternatives.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_branches: Vec<HelperBranch>,
    /// Predicate-qualified alternatives behind an inline-conditional
    /// `kind:`. An IR-internal enrichment: attached at use-tagging time
    /// (the selecting locals resolve only in template scope) and consumed
    /// by the contract-signal builder's per-row kind concretization. It
    /// remains as non-serialized provenance so emission can distinguish an
    /// exact selected arm from a selector-independent concrete resource.
    #[serde(skip)]
    pub kind_branches: Vec<KindBranch>,
    /// The values path whose value is the rendered kind
    /// (`kind: {{ .Values.workload.kind }}`), when the kind and its
    /// candidates are the literals the template compares that path with.
    /// Non-serialized provenance: only a guard comparing this path to a
    /// candidate selects that kind.
    #[serde(skip)]
    pub kind_selector: Option<ValuesPath>,
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
            kind_selector: None,
        }
    }
}
