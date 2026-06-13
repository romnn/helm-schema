use helm_schema_ast::{DefineIndex, HelmAst, Literal, TemplateExpr, parse_action_expressions};

use crate::ResourceRef;
use crate::capability_branch::{CapabilityGuard, HelperBranch, HelperBranchBody, decode_guard};
use crate::helper_eval::{HelperOutput, helper_evaluate};

/// AST-driven detector for Kubernetes resource identity.
///
/// The detector only reads manifest structure: top-level `apiVersion` / `kind`
/// mapping pairs, structural Helm control-flow nodes that wrap those pairs, and
/// helper calls in `apiVersion` values that statically evaluate to literal
/// outputs. It preserves typed capability branches so the K8s lookup layer can
/// choose the runtime-live branch instead of flattening mutually-exclusive
/// alternatives.
pub(crate) struct AstResourceDetector<'a> {
    defines: &'a DefineIndex,
}

impl<'a> AstResourceDetector<'a> {
    #[must_use]
    pub(crate) fn new(defines: &'a DefineIndex) -> Self {
        Self { defines }
    }

    /// Detect the resource identity for one manifest document subtree.
    ///
    /// Multi-document template sources are split before this method is called.
    /// Keeping that boundary outside the detector avoids mixing `apiVersion`
    /// candidates from unrelated YAML documents.
    #[must_use]
    pub(crate) fn detect(&self, ast: &HelmAst) -> Option<ResourceRef> {
        let mut state = ResourceState::default();
        self.scan_node(ast, &mut state, true);
        state.resource()
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
                        if let Some(output) = self.api_version_output(value.as_deref()) {
                            state.record_api_version_output(output);
                        }
                    }
                    "kind" => {
                        if state.kind.is_none()
                            && let Some(value) = value.as_deref().and_then(scalar_text)
                            && !value.is_empty()
                        {
                            state.kind = Some(value.to_string());
                        }
                    }
                    _ => {}
                }
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                if capture_branches
                    && is_capability_guard(cond)
                    && let Some(branches) = self.inline_api_version_branches(node)
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

    fn inline_api_version_branches(&self, node: &HelmAst) -> Option<Vec<HelperBranch>> {
        let branches = self.inline_api_version_branches_inner(node)?;
        if branches.is_empty() {
            None
        } else {
            Some(branches)
        }
    }

    fn inline_api_version_branches_inner(&self, node: &HelmAst) -> Option<Vec<HelperBranch>> {
        let HelmAst::If {
            cond,
            then_branch,
            else_branch,
        } = node
        else {
            return None;
        };
        let guard = decode_guard(cond);
        if !matches!(
            guard,
            CapabilityGuard::Has { .. } | CapabilityGuard::NotHas { .. }
        ) {
            return None;
        }

        let mut branches = Vec::new();
        branches.push(HelperBranch {
            guard: Some(guard),
            body: self.api_version_branch_body(then_branch),
        });

        if let [nested @ HelmAst::If { .. }] = else_branch.as_slice()
            && let Some(nested_branches) = self.inline_api_version_branches_inner(nested)
        {
            branches.extend(nested_branches);
        } else if !else_branch.is_empty() {
            branches.push(HelperBranch {
                guard: None,
                body: self.api_version_branch_body(else_branch),
            });
        }

        branches.retain(|branch| !branch.body.is_empty());
        if branches.is_empty() {
            None
        } else {
            Some(branches)
        }
    }

    fn api_version_branch_body(&self, items: &[HelmAst]) -> HelperBranchBody {
        let mut literals = Vec::new();
        let mut nested = Vec::new();
        for item in items {
            self.collect_api_version_outputs(item, &mut literals, &mut nested);
        }
        if nested.is_empty() {
            return HelperBranchBody::literals(dedup_preserve_order(literals));
        }

        let literals = dedup_preserve_order(literals);
        if !literals.is_empty() {
            nested.insert(
                0,
                HelperBranch {
                    guard: None,
                    body: HelperBranchBody::literals(literals),
                },
            );
        }
        HelperBranchBody::Nested { branches: nested }
    }

    fn collect_api_version_outputs(
        &self,
        node: &HelmAst,
        literals: &mut Vec<String>,
        nested: &mut Vec<HelperBranch>,
    ) {
        match node {
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                for item in items {
                    self.collect_api_version_outputs(item, literals, nested);
                }
            }
            HelmAst::Pair { key, value } => {
                if scalar_text(key) == Some("apiVersion")
                    && let Some(output) = self.api_version_output(value.as_deref())
                {
                    match output {
                        HelperOutput::Literals(values) => literals.extend(values),
                        HelperOutput::Branched { branches } => nested.extend(branches),
                    }
                }
            }
            HelmAst::If { .. } => {
                if let Some(branches) = self.inline_api_version_branches_inner(node) {
                    nested.extend(branches);
                }
            }
            HelmAst::Range {
                body, else_branch, ..
            }
            | HelmAst::With {
                body, else_branch, ..
            } => {
                for item in body.iter().chain(else_branch) {
                    self.collect_api_version_outputs(item, literals, nested);
                }
            }
            HelmAst::Block { body, .. } => {
                for item in body {
                    self.collect_api_version_outputs(item, literals, nested);
                }
            }
            HelmAst::Define { .. }
            | HelmAst::Sequence { .. }
            | HelmAst::Scalar { .. }
            | HelmAst::HelmExpr { .. }
            | HelmAst::HelmComment { .. } => {}
        }
    }

    fn api_version_output(&self, value: Option<&HelmAst>) -> Option<HelperOutput> {
        match value? {
            HelmAst::Scalar { text } => {
                let value = text.trim();
                if value.is_empty() {
                    None
                } else {
                    Some(HelperOutput::Literals(vec![value.to_string()]))
                }
            }
            HelmAst::HelmExpr { text } => self.helper_api_version_output(text),
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                for item in items {
                    if let Some(output) = self.api_version_output(Some(item)) {
                        return Some(output);
                    }
                }
                None
            }
            HelmAst::Pair { value, .. } => self.api_version_output(value.as_deref()),
            node @ HelmAst::If { .. } => self
                .inline_api_version_branches(node)
                .map(|branches| HelperOutput::Branched { branches }),
            HelmAst::Sequence { .. }
            | HelmAst::Range { .. }
            | HelmAst::With { .. }
            | HelmAst::Define { .. }
            | HelmAst::Block { .. }
            | HelmAst::HelmComment { .. } => None,
        }
    }

    fn helper_api_version_output(&self, text: &str) -> Option<HelperOutput> {
        let mut combined = ResourceState::default();
        for name in helper_call_names(text) {
            combined.record_api_version_output(helper_evaluate(&name, self.defines));
        }
        if combined.api_versions.is_empty() && combined.api_version_branches.is_empty() {
            None
        } else if combined.api_version_branches.is_empty() {
            Some(HelperOutput::Literals(combined.api_versions))
        } else {
            Some(HelperOutput::Branched {
                branches: combined.api_version_branches,
            })
        }
    }
}

#[derive(Default)]
struct ResourceState {
    kind: Option<String>,
    api_versions: Vec<String>,
    multi_branch: bool,
    api_version_branches: Vec<HelperBranch>,
}

impl ResourceState {
    fn record_api_version_output(&mut self, output: HelperOutput) {
        match output {
            HelperOutput::Literals(literals) => {
                if literals.len() > 1 {
                    self.multi_branch = true;
                }
                for literal in literals {
                    self.insert_api_version(literal);
                }
            }
            HelperOutput::Branched { branches } => self.record_api_version_branches(branches),
        }
    }

    fn record_api_version_branches(&mut self, branches: Vec<HelperBranch>) {
        if branches.is_empty() {
            return;
        }
        self.multi_branch = true;
        for branch in &branches {
            for literal in branch.body.all_literals() {
                self.insert_api_version(literal);
            }
        }
        self.api_version_branches.extend(branches);
    }

    fn insert_api_version(&mut self, value: String) {
        if !value.is_empty() && !self.api_versions.contains(&value) {
            self.api_versions.push(value);
        }
    }

    fn resource(self) -> Option<ResourceRef> {
        let kind = self.kind?;
        let (api_version, api_version_candidates) = if self.multi_branch {
            (String::new(), self.api_versions)
        } else {
            let mut versions = self.api_versions;
            let primary = versions.first().cloned().unwrap_or_default();
            versions.retain(|version| version != &primary);
            (primary, versions)
        };
        Some(ResourceRef {
            api_version,
            kind,
            api_version_candidates,
            api_version_branches: self.api_version_branches,
        })
    }
}

fn scalar_text(node: &HelmAst) -> Option<&str> {
    match node {
        HelmAst::Scalar { text } => Some(text.trim()),
        _ => None,
    }
}

fn is_capability_guard(cond: &str) -> bool {
    matches!(
        decode_guard(cond),
        CapabilityGuard::Has { .. } | CapabilityGuard::NotHas { .. }
    )
}

fn helper_call_names(text: &str) -> Vec<String> {
    let action_text = format!("{{{{ {text} }}}}");
    let mut out = Vec::new();
    for expr in parse_action_expressions(&action_text) {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            if !matches!(function.as_str(), "include" | "template") {
                return;
            }
            let Some(TemplateExpr::Literal(Literal::String(name) | Literal::RawString(name))) =
                args.first()
            else {
                return;
            };
            if !name.is_empty() && !out.contains(name) {
                out.push(name.clone());
            }
        });
    }
    out
}

fn dedup_preserve_order(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
    use indoc::indoc;

    use super::AstResourceDetector;
    use crate::capability_branch::{CapabilityGuard, HelperBranchBody};

    fn detect(src: &str, defines: &DefineIndex) -> Option<crate::ResourceRef> {
        let ast = TreeSitterParser.parse(src).expect("parse template");
        AstResourceDetector::new(defines).detect(&ast)
    }

    #[test]
    fn detects_kind_before_api_version() {
        let resource = detect(
            indoc! {r#"
                kind: NetworkPolicy
                apiVersion: networking.k8s.io/v1
                metadata:
                  name: example
            "#},
            &DefineIndex::new(),
        )
        .expect("resource");

        assert_eq!(resource.kind, "NetworkPolicy");
        assert_eq!(resource.api_version, "networking.k8s.io/v1");
    }

    #[test]
    fn resolves_helper_returned_api_version() {
        let helpers = indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- print "apps/v1" -}}
            {{- end -}}
        "#};
        let mut defines = DefineIndex::new();
        defines
            .add_source(&TreeSitterParser, helpers)
            .expect("helpers");
        let resource = detect(
            indoc! {r#"
                apiVersion: {{ template "x.apiVersion" . }}
                kind: Deployment
                metadata:
                  name: example
            "#},
            &defines,
        )
        .expect("resource");

        assert_eq!(resource.kind, "Deployment");
        assert_eq!(resource.api_version, "apps/v1");
        assert!(resource.api_version_candidates.is_empty());
    }

    #[test]
    fn preserves_inline_capability_branches() {
        let resource = detect(
            indoc! {r#"
                {{- if .Capabilities.APIVersions.Has "policy/v1" }}
                apiVersion: policy/v1
                {{- else }}
                apiVersion: policy/v1beta1
                {{- end }}
                kind: PodDisruptionBudget
                metadata:
                  name: example
            "#},
            &DefineIndex::new(),
        )
        .expect("resource");

        assert_eq!(resource.kind, "PodDisruptionBudget");
        assert_eq!(resource.api_version, "");
        assert_eq!(
            resource.api_version_candidates,
            vec!["policy/v1".to_string(), "policy/v1beta1".to_string()]
        );
        assert_eq!(resource.api_version_branches.len(), 2);
        assert_eq!(
            resource.api_version_branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "policy/v1".to_string()
            })
        );
        assert_eq!(
            resource.api_version_branches[1].body,
            HelperBranchBody::literals(vec!["policy/v1beta1".to_string()])
        );
    }

    #[test]
    fn mixed_literal_and_nested_branch_preserves_nested_guards() {
        let resource = detect(
            indoc! {r#"
                {{- if .Capabilities.APIVersions.Has "policy/v1" }}
                apiVersion: policy/v1
                {{- if .Capabilities.APIVersions.Has "policy/v1/PodDisruptionBudget" }}
                apiVersion: policy/v1
                {{- else }}
                apiVersion: policy/v1beta1
                {{- end }}
                {{- else }}
                apiVersion: policy/v1beta1
                {{- end }}
                kind: PodDisruptionBudget
            "#},
            &DefineIndex::new(),
        )
        .expect("resource");

        let HelperBranchBody::Nested { branches } = &resource.api_version_branches[0].body else {
            panic!("expected nested branch body");
        };
        assert_eq!(branches.len(), 3);
        assert_eq!(
            branches[0].body,
            HelperBranchBody::literals(vec!["policy/v1".to_string()])
        );
        assert_eq!(
            branches[1].guard,
            Some(CapabilityGuard::Has {
                api: "policy/v1/PodDisruptionBudget".to_string()
            })
        );
        assert_eq!(
            branches[2].body,
            HelperBranchBody::literals(vec!["policy/v1beta1".to_string()])
        );
    }

    #[test]
    fn capability_guard_without_api_version_does_not_create_empty_branch_resource() {
        let resource = detect(
            indoc! {r#"
                {{- if .Capabilities.APIVersions.Has "v1/ConfigMap" }}
                metadata:
                  labels:
                    enabled: "true"
                {{- end }}
                apiVersion: v1
                kind: ConfigMap
            "#},
            &DefineIndex::new(),
        )
        .expect("resource");

        assert_eq!(resource.kind, "ConfigMap");
        assert_eq!(resource.api_version, "v1");
        assert!(resource.api_version_candidates.is_empty());
        assert!(resource.api_version_branches.is_empty());
    }
}
