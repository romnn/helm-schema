mod abstract_eval;
mod abstract_value;
mod assignment_action_plan;
mod binding;
mod bound_helper_call_analysis;
mod bound_value_analysis;
mod condition_action_plan;
mod define_body_cache;
mod eval_effect;
mod eval_env;
mod expr_eval;
mod expression_analysis;
mod fragment_binding_eval;
mod fragment_expr_eval;
mod fragment_scope_eval;
mod helper_analysis;
mod helper_binding_eval;
pub mod helper_eval;
mod helper_fragment_output_uses;
mod helper_fragment_outputs;
mod helper_output_projection;
mod helper_value_analysis;
mod local_projection;
mod node_action_effect;
mod node_action_kind;
mod node_eval;
mod output_node_context;
mod output_path;
mod output_value_analysis;
mod output_value_emitter;
mod range_action_plan;
mod rendered_yaml_context;
pub mod required_inference;
mod resource_detector;
mod resource_locator;
mod static_file_template;
mod symbolic;
mod template_expr_analysis;
mod template_expr_cache;
mod tree_sitter_utils;
mod value_path_context;
mod value_use_postprocess;
mod walker;
mod yaml_shape;

pub use abstract_eval::{ChartFacts, PathFact, derive_chart_facts, derive_chart_facts_from_ast};
pub use helper_eval::{
    CapabilityGuard, HelperBranch, HelperBranchBody, HelperOutput, helper_evaluate,
    helper_literal_outputs,
};
pub use symbolic::{SymbolicIrContext, SymbolicIrGenerator};
pub use walker::{
    DefineBlock, extract_default_type_hints, extract_define_blocks, extract_helper_calls,
};

use serde::{Deserialize, Serialize};

use helm_schema_ast::{DefineIndex, HelmAst};

// ---------------------------------------------------------------------------
// Core IR types
// ---------------------------------------------------------------------------

/// A single use of a `.Values.*` path in a Helm template.
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

/// Whether a value use produces a single scalar or a YAML fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ValueKind {
    Scalar = 0,
    Fragment = 1,
}

/// Detected Kubernetes resource type (apiVersion + kind).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub api_version: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_version_candidates: Vec<String>,
    /// Typed branch structure when the apiVersion is decided by an
    /// `if Capabilities.APIVersions.Has … else …` chain — either inside
    /// a helper body or inline around the `apiVersion:` line in the
    /// document header.
    ///
    /// The chain layer evaluates each branch's guard against its K8s
    /// version cache (the actual capability oracle) and picks the
    /// first live branch's literals for both resolution and diagnostic
    /// attribution. Without this typed structure, mutually-exclusive
    /// alternatives would have to be treated as peer candidates,
    /// producing one `MissingSchema` per alternative when none resolve
    /// — which is misleading because at runtime exactly ONE branch is
    /// taken.
    ///
    /// Empty = no decodable branch structure; callers fall back to
    /// `api_version` + `api_version_candidates`.
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
    /// Body of `range .Values.X` / `range .foo` block — the referenced path is
    /// being iterated as a collection, not interpreted as a boolean-valued
    /// scalar. This should not contribute a boolean type hint downstream.
    Range { path: String },
    /// Body of `with .Values.X` block — distinguishes header binding from
    /// `if`-style truthy checks. The bound path is null-tolerant by
    /// construction (`with nil` skips the body).
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
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Generates IR (`Vec<ValueUse>`) from a parsed Helm+YAML AST.
pub trait IrGenerator {
    fn generate(&self, src: &str, ast: &HelmAst, defines: &DefineIndex) -> Vec<ValueUse>;
}

#[cfg(test)]
mod tests;
