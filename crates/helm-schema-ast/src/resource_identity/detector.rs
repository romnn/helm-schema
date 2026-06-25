use crate::{DefineIndex, HelmAst, HelperOutputEvaluator};

use super::state::ResourceState;
use helm_schema_core::ResourceRef;

/// AST-driven detector for Kubernetes resource identity.
///
/// The detector only reads manifest structure: top-level `apiVersion` / `kind`
/// mapping pairs, structural Helm control-flow nodes that wrap those pairs, and
/// helper calls in `apiVersion` values that statically evaluate to literal
/// outputs. It preserves typed capability branches so the K8s lookup layer can
/// choose the runtime-live branch instead of flattening mutually-exclusive
/// alternatives.
pub struct ResourceIdentityDetector<'a> {
    defines: &'a DefineIndex,
}

impl<'a> ResourceIdentityDetector<'a> {
    #[must_use]
    pub fn new(defines: &'a DefineIndex) -> Self {
        Self { defines }
    }

    /// Detect the resource identity for one manifest document subtree.
    ///
    /// Multi-document template sources are split before this method is called.
    /// Keeping that boundary outside the detector avoids mixing `apiVersion`
    /// candidates from unrelated YAML documents.
    #[must_use]
    pub fn detect(&self, ast: &HelmAst) -> Option<ResourceRef> {
        let mut state = ResourceState::default();
        self.scan_node(ast, &mut state, true);
        state.into_resource()
    }

    fn scan_items(&self, items: &[HelmAst], state: &mut ResourceState, capture_branches: bool) {
        for item in items {
            self.scan_node(item, state, capture_branches);
        }
    }

    fn scan_node(&self, node: &HelmAst, state: &mut ResourceState, capture_branches: bool) {
        match node {
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                self.scan_items(items, state, capture_branches);
            }
            HelmAst::Pair { key, value } => {
                let Some(key_text) = scalar_text(key) else {
                    return;
                };
                match key_text {
                    "apiVersion" => {
                        if let Some(output) = HelperOutputEvaluator::new()
                            .evaluate_ast_value(value.as_deref(), self.defines)
                        {
                            state.record_api_version_output(output);
                        }
                    }
                    "kind" => {
                        if let Some(value) = value.as_deref().and_then(scalar_text) {
                            state.set_kind_if_empty(value);
                        }
                    }
                    _ => {}
                }
            }
            HelmAst::If {
                then_branch,
                else_branch,
                ..
            } => {
                if capture_branches
                    && let Some(branches) = HelperOutputEvaluator::new()
                        .evaluate_keyed_inline_branches(node, "apiVersion", self.defines)
                {
                    state.record_api_version_branches(branches);
                    self.scan_items(then_branch, state, false);
                    self.scan_items(else_branch, state, false);
                    return;
                }
                self.scan_items(then_branch, state, capture_branches);
                self.scan_items(else_branch, state, capture_branches);
            }
            HelmAst::Range {
                body, else_branch, ..
            }
            | HelmAst::With {
                body, else_branch, ..
            } => {
                self.scan_items(body, state, capture_branches);
                self.scan_items(else_branch, state, capture_branches);
            }
            HelmAst::Block { body, .. } => {
                self.scan_items(body, state, capture_branches);
            }
            HelmAst::Define { .. }
            | HelmAst::Sequence { .. }
            | HelmAst::Scalar { .. }
            | HelmAst::HelmExpr { .. }
            | HelmAst::HelmComment { .. } => {}
        }
    }
}

fn scalar_text(node: &HelmAst) -> Option<&str> {
    match node {
        HelmAst::Scalar { text } => Some(text.trim()),
        _ => None,
    }
}
