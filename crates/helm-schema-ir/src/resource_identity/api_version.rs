use helm_schema_ast::{DefineIndex, HelmAst, TemplateHeader};

use crate::capability_branch::{
    CapabilityGuard, HelperBranch, HelperBranchBody, decode_guard, decode_guard_expr,
};

use super::helper_output::{HelperOutput, HelperOutputEvaluator};

pub(super) struct ApiVersionOutputDetector<'a> {
    defines: &'a DefineIndex,
}

impl<'a> ApiVersionOutputDetector<'a> {
    pub(super) fn new(defines: &'a DefineIndex) -> Self {
        Self { defines }
    }

    pub(super) fn is_capability_guard(&self, condition: &TemplateHeader) -> bool {
        matches!(
            decode_guard_expr(condition.expr(), condition.raw())
                .unwrap_or_else(|| decode_guard(condition.raw())),
            CapabilityGuard::Has { .. } | CapabilityGuard::NotHas { .. }
        )
    }

    pub(super) fn inline_branches(&self, node: &HelmAst) -> Option<Vec<HelperBranch>> {
        let branches = self.inline_branches_inner(node)?;
        if branches.is_empty() {
            None
        } else {
            Some(branches)
        }
    }

    fn inline_branches_inner(&self, node: &HelmAst) -> Option<Vec<HelperBranch>> {
        let HelmAst::If {
            condition,
            then_branch,
            else_branch,
        } = node
        else {
            return None;
        };
        let guard = decode_guard_expr(condition.expr(), condition.raw())
            .unwrap_or_else(|| decode_guard(condition.raw()));
        if !matches!(
            guard,
            CapabilityGuard::Has { .. } | CapabilityGuard::NotHas { .. }
        ) {
            return None;
        }

        let mut branches = Vec::new();
        branches.push(HelperBranch {
            guard: Some(guard),
            body: self.branch_body(then_branch),
        });

        if let [nested @ HelmAst::If { .. }] = else_branch.as_slice()
            && let Some(nested_branches) = self.inline_branches_inner(nested)
        {
            branches.extend(nested_branches);
        } else if !else_branch.is_empty() {
            branches.push(HelperBranch {
                guard: None,
                body: self.branch_body(else_branch),
            });
        }

        branches.retain(|branch| !branch.body.is_empty());
        if branches.is_empty() {
            None
        } else {
            Some(branches)
        }
    }

    fn branch_body(&self, items: &[HelmAst]) -> HelperBranchBody {
        let mut literals = Vec::new();
        let mut nested = Vec::new();
        for item in items {
            self.collect_outputs(item, &mut literals, &mut nested);
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

    fn collect_outputs(
        &self,
        node: &HelmAst,
        literals: &mut Vec<String>,
        nested: &mut Vec<HelperBranch>,
    ) {
        match node {
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                for item in items {
                    self.collect_outputs(item, literals, nested);
                }
            }
            HelmAst::Pair { key, value } => {
                if scalar_text(key) == Some("apiVersion")
                    && let Some(output) = self.output(value.as_deref())
                {
                    match output {
                        HelperOutput::Literals(values) => literals.extend(values),
                        HelperOutput::Branched { branches } => nested.extend(branches),
                    }
                }
            }
            HelmAst::If { .. } => {
                if let Some(branches) = self.inline_branches_inner(node) {
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
                    self.collect_outputs(item, literals, nested);
                }
            }
            HelmAst::Block { body, .. } => {
                for item in body {
                    self.collect_outputs(item, literals, nested);
                }
            }
            HelmAst::Define { .. }
            | HelmAst::Sequence { .. }
            | HelmAst::Scalar { .. }
            | HelmAst::HelmExpr { .. }
            | HelmAst::HelmComment { .. } => {}
        }
    }

    pub(super) fn output(&self, value: Option<&HelmAst>) -> Option<HelperOutput> {
        match value? {
            HelmAst::Scalar { text } => {
                let value = text.trim();
                if value.is_empty() {
                    None
                } else {
                    Some(HelperOutput::Literals(vec![value.to_string()]))
                }
            }
            HelmAst::HelmExpr { action } => {
                HelperOutputEvaluator::new().evaluate_action(action, self.defines)
            }
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                for item in items {
                    if let Some(output) = self.output(Some(item)) {
                        return Some(output);
                    }
                }
                None
            }
            HelmAst::Pair { value, .. } => self.output(value.as_deref()),
            node @ HelmAst::If { .. } => self
                .inline_branches(node)
                .map(|branches| HelperOutput::Branched { branches }),
            HelmAst::Sequence { .. }
            | HelmAst::Range { .. }
            | HelmAst::With { .. }
            | HelmAst::Define { .. }
            | HelmAst::Block { .. }
            | HelmAst::HelmComment { .. } => None,
        }
    }
}

pub(super) fn scalar_text(node: &HelmAst) -> Option<&str> {
    match node {
        HelmAst::Scalar { text } => Some(text.trim()),
        _ => None,
    }
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
