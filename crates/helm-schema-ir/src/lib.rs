mod walker;

pub use walker::DefaultIrGenerator;

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
}

impl Guard {
    /// Return all `.Values.*` paths referenced by this guard.
    pub fn value_paths(&self) -> Vec<&str> {
        match self {
            Guard::Truthy { path } | Guard::Not { path } | Guard::Eq { path, .. } => {
                vec![path.as_str()]
            }
            Guard::Or { paths } => paths.iter().map(|s| s.as_str()).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Generates IR (`Vec<ValueUse>`) from a parsed Helm+YAML AST.
pub trait IrGenerator {
    fn generate(&self, ast: &HelmAst, defines: &DefineIndex) -> Vec<ValueUse>;
}

/// Detects the Kubernetes resource type from an AST node.
///
/// The default walker uses inline detection (tracking `apiVersion`/`kind` pairs
/// during the walk). This trait allows alternative strategies.
pub trait ResourceDetector {
    fn detect(&self, ast: &HelmAst) -> Option<ResourceRef>;
}

/// Default resource detector that scans top-level mapping pairs for
/// `apiVersion` and `kind` scalars.
pub struct DefaultResourceDetector;

impl ResourceDetector for DefaultResourceDetector {
    fn detect(&self, ast: &HelmAst) -> Option<ResourceRef> {
        let mut api_version = None;
        let mut kind = None;
        scan_for_resource(ast, &mut api_version, &mut kind);
        match (api_version, kind) {
            (Some(av), Some(k)) => Some(ResourceRef {
                api_version: av,
                kind: k,
            }),
            // Many Helm charts use `{{ template "..." }}` for apiVersion,
            // so we still detect the resource if only `kind` is found.
            (None, Some(k)) => Some(ResourceRef {
                api_version: String::new(),
                kind: k,
            }),
            _ => None,
        }
    }
}

fn scan_for_resource(node: &HelmAst, api_version: &mut Option<String>, kind: &mut Option<String>) {
    match node {
        HelmAst::Document { items } | HelmAst::Mapping { items } => {
            for item in items {
                scan_for_resource(item, api_version, kind);
            }
        }
        HelmAst::Pair { key, value } => {
            if let HelmAst::Scalar { text: key_text } = key.as_ref() {
                if let Some(val_text) = value.as_ref().and_then(|v| match v.as_ref() {
                    HelmAst::Scalar { text } => Some(text.as_str()),
                    _ => None,
                }) {
                    if key_text == "apiVersion" {
                        *api_version = Some(val_text.to_string());
                    } else if key_text == "kind" {
                        *kind = Some(val_text.to_string());
                    }
                }
            }
        }
        HelmAst::If { then_branch, .. } => {
            for item in then_branch {
                scan_for_resource(item, api_version, kind);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
