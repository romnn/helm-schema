use serde::{Deserialize, Serialize};

use crate::capability_branch::HelperBranch;

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

/// YAML path in the rendered manifest, e.g. `["metadata", "name"]`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct YamlPath(pub Vec<String>);

/// Whether a value use produces a full scalar, part of a scalar, or a YAML fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ValueKind {
    Scalar = 0,
    PartialScalar = 1,
    Fragment = 2,
}

/// Detected Kubernetes resource type (apiVersion + kind).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub api_version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_candidates: Vec<String>,
    /// Typed branch structure when the apiVersion is decided by an
    /// `if Capabilities.APIVersions.Has ... else ...` chain: either inside
    /// a helper body or inline around the `apiVersion:` line in the
    /// document header.
    ///
    /// The chain layer evaluates each branch's guard against its K8s
    /// version cache (the actual capability oracle) and picks the
    /// first live branch's literals for both resolution and diagnostic
    /// attribution. Without this typed structure, mutually-exclusive
    /// alternatives would have to be treated as peer candidates,
    /// producing one `MissingSchema` per alternative when none resolve
    /// because at runtime exactly one branch is taken.
    ///
    /// Empty means no decodable branch structure; callers fall back to
    /// `api_version` plus `api_version_candidates`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_branches: Vec<HelperBranch>,
}

/// A guard condition from an `if`, `with`, or `range` block.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Guard {
    /// Simple truthy check: `if .Values.X`
    Truthy { path: String },
    /// Negated truthy check: `if not .Values.X`
    Not { path: String },
    /// Equality check: `if eq .Values.X "value"`
    Eq { path: String, value: String },
    /// Disjunction: `if or .Values.A .Values.B`
    Or { paths: Vec<String> },
    /// Body of `range .Values.X` / `range .foo` block. The referenced path is
    /// being iterated as a collection, not interpreted as a boolean-valued
    /// scalar. This should not contribute a boolean type hint downstream.
    Range { path: String },
    /// Body of `with .Values.X` block. This distinguishes header binding from
    /// `if`-style truthy checks. The bound path is null-tolerant by
    /// construction because `with nil` skips the body.
    With { path: String },
    /// Rendered via a `default ... <path>` fallback, either in prefix form
    /// (`default "x" .Values.X`) or pipeline form (`.Values.X | default "x"`).
    ///
    /// This is stronger than a plain truthy guard: the template explicitly
    /// substitutes a fallback when the path is empty/nil, so `null` is an
    /// accepted chart input for that render site even when `values.yaml` ships
    /// a non-null default.
    Default { path: String },
    /// A `typeIs "<json type>" <path>` check in template logic.
    ///
    /// This is not a truthiness guard. It is a structural type declaration:
    /// helpers such as Bitnami's `common.tplvalues.render` explicitly branch on
    /// `typeIs "string" .value`, so callers may supply that values path as a
    /// string even when another branch renders it as a YAML object fragment.
    TypeIs { path: String, schema_type: String },
}

impl Guard {
    /// Return all `.Values.*` paths referenced by this guard.
    #[must_use]
    pub fn value_paths(&self) -> Vec<&str> {
        match self {
            Guard::Truthy { path }
            | Guard::Not { path }
            | Guard::Eq { path, .. }
            | Guard::Range { path }
            | Guard::With { path }
            | Guard::Default { path }
            | Guard::TypeIs { path, .. } => {
                vec![path.as_str()]
            }
            Guard::Or { paths } => paths.iter().map(std::string::String::as_str).collect(),
        }
    }

    /// Rewrite value paths carried by this guard.
    #[must_use]
    pub fn map_value_paths<F>(self, map: &mut F) -> Self
    where
        F: FnMut(&str) -> String,
    {
        match self {
            Guard::Truthy { path } => Guard::Truthy { path: map(&path) },
            Guard::Not { path } => Guard::Not { path: map(&path) },
            Guard::Eq { path, value } => Guard::Eq {
                path: map(&path),
                value,
            },
            Guard::Or { paths } => Guard::Or {
                paths: paths.into_iter().map(|path| map(&path)).collect(),
            },
            Guard::Range { path } => Guard::Range { path: map(&path) },
            Guard::With { path } => Guard::With { path: map(&path) },
            Guard::Default { path } => Guard::Default { path: map(&path) },
            Guard::TypeIs { path, schema_type } => Guard::TypeIs {
                path: map(&path),
                schema_type,
            },
        }
    }
}
