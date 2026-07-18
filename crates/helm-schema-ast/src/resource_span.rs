//! Manifest resource spans: byte ranges of one template source that belong
//! to a detected Kubernetes resource, with the path prefix List-envelope
//! items strip from emitted paths.

use helm_schema_core::ResourceRef;

#[derive(Clone, Debug)]
pub struct ResourceSpan {
    pub start: usize,
    pub end: usize,
    pub resource: ResourceRef,
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
    pub condition: Option<String>,
    pub kind: String,
}
