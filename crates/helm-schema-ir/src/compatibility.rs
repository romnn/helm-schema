use serde::{Deserialize, Serialize};

use crate::{Guard, ResourceRef, ValueKind, YamlPath};

/// Serialized inspection row for one observed `.Values.*` path.
///
/// The semantic interpreter produces `ContractIr` / `ContractUse` internally.
/// `ValueUse` is kept as a stable fixture and external-tooling projection
/// format, not as the production contract artifact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ValueUse {
    /// The `.Values.*` sub-path, e.g. `"metrics.enabled"`.
    pub source_expr: String,
    /// The YAML path where this value is placed in the rendered manifest.
    pub path: YamlPath,
    /// Whether this produces a scalar or a YAML fragment.
    pub kind: ValueKind,
    /// Guard conditions (from `if`/`with`/`range`) active when this use appears.
    pub guards: Vec<Guard>,
    /// The Kubernetes resource type detected in context, if any.
    pub resource: Option<ResourceRef>,
}
