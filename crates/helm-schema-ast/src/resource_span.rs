//! Manifest resource spans: byte ranges of one template source that belong
//! to a detected Kubernetes resource, with the path prefix List-envelope
//! items strip from emitted paths.

use helm_schema_core::ResourceRef;

/// Source range and resource identity for one rendered manifest document.
#[derive(Clone, Debug)]
pub struct ResourceSpan {
    /// Inclusive byte offset where the resource begins.
    pub start: usize,
    /// Exclusive byte offset where the resource ends.
    pub end: usize,
    /// Kubernetes identity recovered for the resource.
    pub resource: ResourceRef,
    /// Structural prefix removed from paths inside a `kind: List` envelope.
    pub path_prefix: Vec<String>,
    /// Raw per-arm sources of an inline-conditional `kind:` value, in arm
    /// order. The guard texts are unresolved template conditions — the
    /// selecting locals only bind in template scope, so the evaluator
    /// lowers them into [`helm_schema_core::KindBranch`] predicates at
    /// use-tagging time.
    pub kind_branch_sources: Vec<KindBranchSource>,
}

/// One arm of an inline-conditional `kind:` scalar
/// (`kind: {{ if $stateful }}StatefulSet{{ else }}Deployment{{ end }}`):
/// the arm's raw condition text (`None` for the trailing `else`) and its
/// kind literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindBranchSource {
    /// Raw selecting condition, or `None` for the trailing `else` arm.
    pub condition: Option<String>,
    /// Kubernetes kind literal emitted by the arm.
    pub kind: String,
}
