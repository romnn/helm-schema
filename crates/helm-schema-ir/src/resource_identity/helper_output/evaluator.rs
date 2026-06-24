use std::collections::HashSet;

use helm_schema_ast::{DefineIndex, HelmAst, TemplateAction, TemplateExpr, TemplateHeader};

use crate::capability_branch::{decode_guard, decode_guard_expr};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, literal_helper_call_callee};
use crate::{CapabilityGuard, HelperBranch, HelperBranchBody};

use super::{HelperOutput, MAX_RECURSION_DEPTH};

pub(crate) struct HelperOutputEvaluator {
    seen: HashSet<String>,
}

impl HelperOutputEvaluator {
    pub(crate) fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    pub(crate) fn evaluate_ast_value(
        mut self,
        value: Option<&HelmAst>,
        helpers: &DefineIndex,
    ) -> Option<HelperOutput> {
        self.evaluate_value_node(value?, helpers, 0)
    }

    pub(crate) fn evaluate_keyed_inline_branches(
        mut self,
        node: &HelmAst,
        key: &str,
        helpers: &DefineIndex,
    ) -> Option<Vec<HelperBranch>> {
        self.extract_keyed_inline_branches(node, key, helpers, 0)
    }

    fn evaluate_value_node(
        &mut self,
        node: &HelmAst,
        helpers: &DefineIndex,
        depth: usize,
    ) -> Option<HelperOutput> {
        if depth >= MAX_RECURSION_DEPTH {
            return None;
        }
        match node {
            HelmAst::Scalar { text } => {
                let value = text.trim();
                if value.is_empty() {
                    None
                } else {
                    Some(HelperOutput::Literals(vec![value.to_string()]))
                }
            }
            HelmAst::HelmExpr { action } => {
                self.evaluate_action_at_depth(action, helpers, depth + 1)
            }
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                for item in items {
                    if let Some(output) = self.evaluate_value_node(item, helpers, depth + 1) {
                        return Some(output);
                    }
                }
                None
            }
            HelmAst::Pair { value, .. } => {
                self.evaluate_value_node(value.as_deref()?, helpers, depth + 1)
            }
            node @ HelmAst::If { .. } => self
                .extract_top_level_branches(std::slice::from_ref(node), helpers, depth)
                .map(|branches| HelperOutput::Branched { branches }),
            HelmAst::Sequence { .. }
            | HelmAst::Range { .. }
            | HelmAst::With { .. }
            | HelmAst::Define { .. }
            | HelmAst::Block { .. }
            | HelmAst::HelmComment { .. } => None,
        }
    }

    /// Try to project the helper body as a top-level if/elif/else chain.
    ///
    /// Returns `Some(branches)` when the body is one of:
    ///   - exactly one If node (optionally surrounded by whitespace-only
    ///     Scalars and HelmComments), with at least one branch yielding
    ///     literals and at least one branch carrying a decoded
    ///     `CapabilityGuard::Has` / `NotHas` guard; or
    ///   - a lone `{{ template "X" . }}` / `{{ include "X" . }}` call
    ///     (optionally surrounded by whitespace-only Scalars and
    ///     HelmComments) whose callee `X` itself resolves to typed
    ///     branches.
    ///
    /// Returns `None` when the body has mixed content (literal prefixes,
    /// multiple Ifs at the same level, a helper call mixed with other
    /// content, ...). Those cases fall through to the flat `Literals`
    /// representation via `collect_literals`.
    fn extract_top_level_branches(
        &mut self,
        body: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
    ) -> Option<Vec<HelperBranch>> {
        if depth >= MAX_RECURSION_DEPTH {
            return None;
        }
        let mut if_node: Option<&HelmAst> = None;
        let mut lone_helper_call: Option<String> = None;
        for node in body {
            match node {
                HelmAst::Scalar { text } if text.trim().is_empty() => continue,
                HelmAst::HelmComment { .. } => continue,
                HelmAst::If { .. } => {
                    if if_node.is_some() || lone_helper_call.is_some() {
                        return None;
                    }
                    if_node = Some(node);
                }
                HelmAst::HelmExpr { action } => {
                    if if_node.is_some() || lone_helper_call.is_some() {
                        return None;
                    }
                    let callee = lone_helper_call_callee(action)?;
                    lone_helper_call = Some(callee);
                }
                _ => return None,
            }
        }

        if let Some(callee) = lone_helper_call {
            return self
                .with_helper_body(&callee, helpers, |this, body| {
                    this.extract_top_level_branches(body, helpers, depth + 1)
                })
                .flatten();
        }

        let if_node = if_node?;
        let HelmAst::If {
            condition,
            then_branch,
            else_branch,
        } = if_node
        else {
            unreachable!("if_node is non-None only when matched as If above");
        };
        let mut branches: Vec<HelperBranch> = Vec::new();
        self.collect_if_branches(
            condition,
            then_branch,
            else_branch,
            helpers,
            depth,
            &mut branches,
        );

        let has_decoded_guard = branches.iter().any(|branch| {
            matches!(
                branch.guard,
                Some(CapabilityGuard::Has { .. }) | Some(CapabilityGuard::NotHas { .. })
            )
        });
        let has_lits = branches.iter().any(|branch| !branch.body.is_empty());
        if !has_decoded_guard || !has_lits {
            return None;
        }
        Some(branches)
    }

    pub(super) fn evaluate_body(
        &mut self,
        body: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
    ) -> HelperOutput {
        if let Some(branches) = self.extract_top_level_branches(body, helpers, depth) {
            HelperOutput::Branched { branches }
        } else {
            HelperOutput::Literals(dedup_preserve_order(
                self.collect_literals(body, helpers, depth),
            ))
        }
    }

    fn collect_if_branches(
        &mut self,
        condition: &TemplateHeader,
        then_branch: &[HelmAst],
        else_branch: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
        out: &mut Vec<HelperBranch>,
    ) {
        let guard = decode_guard_expr(condition.expr(), condition.raw())
            .unwrap_or_else(|| decode_guard(condition.raw()));
        out.push(HelperBranch {
            guard: Some(guard),
            body: self.collect_branch_body(then_branch, helpers, depth + 1),
        });
        // Detect elif-chains: an else-branch consisting solely of an If
        // (plus optional whitespace / comments) is the Helm lowering of
        // `{{ else if ... }}`.
        if let Some(nested_if) = lone_if_in(else_branch) {
            let HelmAst::If {
                condition,
                then_branch,
                else_branch,
            } = nested_if
            else {
                unreachable!("lone_if_in returns only If nodes");
            };
            self.collect_if_branches(condition, then_branch, else_branch, helpers, depth, out);
        } else if !else_branch.is_empty() {
            let body = self.collect_branch_body(else_branch, helpers, depth + 1);
            if !body.is_empty() {
                out.push(HelperBranch { guard: None, body });
            }
        }
    }

    fn extract_keyed_inline_branches(
        &mut self,
        node: &HelmAst,
        key: &str,
        helpers: &DefineIndex,
        depth: usize,
    ) -> Option<Vec<HelperBranch>> {
        if depth >= MAX_RECURSION_DEPTH {
            return None;
        }
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

        let mut branches = vec![HelperBranch {
            guard: Some(guard),
            body: self.collect_keyed_branch_body(then_branch, key, helpers, depth + 1),
        }];
        if let [nested @ HelmAst::If { .. }] = else_branch.as_slice()
            && let Some(nested_branches) =
                self.extract_keyed_inline_branches(nested, key, helpers, depth + 1)
        {
            branches.extend(nested_branches);
        } else if !else_branch.is_empty() {
            branches.push(HelperBranch {
                guard: None,
                body: self.collect_keyed_branch_body(else_branch, key, helpers, depth + 1),
            });
        }
        branches.retain(|branch| !branch.body.is_empty());
        (!branches.is_empty()).then_some(branches)
    }

    fn collect_keyed_branch_body(
        &mut self,
        nodes: &[HelmAst],
        key: &str,
        helpers: &DefineIndex,
        depth: usize,
    ) -> HelperBranchBody {
        let mut literals = Vec::new();
        let mut nested = Vec::new();
        self.collect_keyed_outputs(nodes, key, helpers, depth, &mut literals, &mut nested);
        let literals = dedup_preserve_order(literals);
        if nested.is_empty() {
            return HelperBranchBody::literals(literals);
        }
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

    fn collect_keyed_outputs(
        &mut self,
        nodes: &[HelmAst],
        key: &str,
        helpers: &DefineIndex,
        depth: usize,
        literals: &mut Vec<String>,
        nested: &mut Vec<HelperBranch>,
    ) {
        if depth >= MAX_RECURSION_DEPTH {
            return;
        }
        for node in nodes {
            match node {
                HelmAst::Document { items } | HelmAst::Mapping { items } => {
                    self.collect_keyed_outputs(items, key, helpers, depth + 1, literals, nested);
                }
                HelmAst::Pair {
                    key: pair_key,
                    value,
                } => {
                    if scalar_text(pair_key) == Some(key)
                        && let Some(value) = value.as_deref()
                        && let Some(output) = self.evaluate_value_node(value, helpers, depth + 1)
                    {
                        match output {
                            HelperOutput::Literals(values) => literals.extend(values),
                            HelperOutput::Branched { branches } => nested.extend(branches),
                        }
                    }
                }
                HelmAst::If { .. } => {
                    if let Some(branches) =
                        self.extract_keyed_inline_branches(node, key, helpers, depth + 1)
                    {
                        nested.extend(branches);
                    }
                }
                HelmAst::Range {
                    body, else_branch, ..
                }
                | HelmAst::With {
                    body, else_branch, ..
                } => {
                    self.collect_keyed_outputs(body, key, helpers, depth + 1, literals, nested);
                    self.collect_keyed_outputs(
                        else_branch,
                        key,
                        helpers,
                        depth + 1,
                        literals,
                        nested,
                    );
                }
                HelmAst::Block { body, .. } => {
                    self.collect_keyed_outputs(body, key, helpers, depth + 1, literals, nested);
                }
                HelmAst::Define { .. }
                | HelmAst::Sequence { .. }
                | HelmAst::Scalar { .. }
                | HelmAst::HelmExpr { .. }
                | HelmAst::HelmComment { .. } => {}
            }
        }
    }

    /// Build a branch payload from a sub-AST. Tries the typed-branched
    /// shape first so guard structure composes through nested bodies.
    fn collect_branch_body(
        &mut self,
        nodes: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
    ) -> HelperBranchBody {
        match self.evaluate_body(nodes, helpers, depth) {
            HelperOutput::Branched { branches } => HelperBranchBody::Nested { branches },
            HelperOutput::Literals(values) => HelperBranchBody::Literals { values },
        }
    }

    fn collect_literals(
        &mut self,
        nodes: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
    ) -> Vec<String> {
        if depth >= MAX_RECURSION_DEPTH {
            return Vec::new();
        }
        let mut out: Vec<String> = Vec::new();
        for node in nodes {
            match node {
                HelmAst::Scalar { text } => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_string());
                    }
                }
                HelmAst::HelmExpr { action } => {
                    if let Some(output) = self.evaluate_action_at_depth(action, helpers, depth) {
                        match output {
                            HelperOutput::Literals(values) => out.extend(values),
                            HelperOutput::Branched { branches } => {
                                let mut seen = HashSet::new();
                                for branch in branches {
                                    branch.body.append_all_literals(&mut out, &mut seen);
                                }
                            }
                        }
                    }
                }
                HelmAst::HelmComment { .. } => {}
                HelmAst::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    out.extend(self.collect_literals(then_branch, helpers, depth + 1));
                    out.extend(self.collect_literals(else_branch, helpers, depth + 1));
                }
                HelmAst::Range {
                    body, else_branch, ..
                }
                | HelmAst::With {
                    body, else_branch, ..
                } => {
                    out.extend(self.collect_literals(body, helpers, depth + 1));
                    out.extend(self.collect_literals(else_branch, helpers, depth + 1));
                }
                HelmAst::Define { body, .. } | HelmAst::Block { body, .. } => {
                    out.extend(self.collect_literals(body, helpers, depth + 1));
                }
                HelmAst::Document { items }
                | HelmAst::Mapping { items }
                | HelmAst::Sequence { items } => {
                    out.extend(self.collect_literals(items, helpers, depth + 1));
                }
                HelmAst::Pair { value, .. } => {
                    if let Some(value) = value.as_deref() {
                        out.extend(self.collect_literals(
                            std::slice::from_ref(value),
                            helpers,
                            depth + 1,
                        ));
                    }
                }
            }
        }
        out
    }

    fn evaluate_action_at_depth(
        &mut self,
        action: &TemplateAction,
        helpers: &DefineIndex,
        depth: usize,
    ) -> Option<HelperOutput> {
        let helper_names = helper_call_names(action);
        if !helper_names.is_empty() {
            let outputs = helper_names.into_iter().filter_map(|name| {
                self.with_helper_body(&name, helpers, |this, body| {
                    this.evaluate_body(body, helpers, depth + 1)
                })
            });
            return combine_helper_outputs(outputs);
        }

        let mut out = Vec::new();
        for expr in action.exprs() {
            out.extend(static_literal_outputs(expr));
        }
        let literals = dedup_preserve_order(out);
        (!literals.is_empty()).then_some(HelperOutput::Literals(literals))
    }

    fn with_helper_body<T>(
        &mut self,
        name: &str,
        helpers: &DefineIndex,
        f: impl FnOnce(&mut Self, &[HelmAst]) -> T,
    ) -> Option<T> {
        if !self.seen.insert(name.to_string()) {
            return None;
        }
        let result = helpers.get(name).map(|body| f(self, body));
        self.seen.remove(name);
        result
    }
}

fn helper_call_names(action: &TemplateAction) -> Vec<String> {
    let mut out = Vec::new();
    for expr in action.exprs() {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            let Some(name) = literal_helper_call_callee(function, args) else {
                return;
            };
            if !name.is_empty() && !out.iter().any(|existing| existing == name) {
                out.push(name.to_string());
            }
        });
    }
    out
}

fn combine_helper_outputs(outputs: impl IntoIterator<Item = HelperOutput>) -> Option<HelperOutput> {
    let mut literals = Vec::new();
    let mut branches = Vec::new();
    for output in outputs {
        match output {
            HelperOutput::Literals(values) => {
                for value in values {
                    if !value.is_empty() && !literals.contains(&value) {
                        literals.push(value);
                    }
                }
            }
            HelperOutput::Branched {
                branches: output_branches,
            } => branches.extend(output_branches),
        }
    }

    if !branches.is_empty() {
        Some(HelperOutput::Branched { branches })
    } else if !literals.is_empty() {
        Some(HelperOutput::Literals(literals))
    } else {
        None
    }
}

/// If `text` is exactly a `template "X" ...` or `include "X" ...` action
/// (possibly with extra args), return `"X"`. Otherwise `None`.
fn lone_helper_call_callee(action: &TemplateAction) -> Option<String> {
    if action.exprs().len() != 1 {
        return None;
    }
    match &action.exprs()[0] {
        TemplateExpr::Call { function, args } => {
            literal_helper_call_callee(function, args).map(str::to_string)
        }
        _ => None,
    }
}

/// Returns the single `If` node nested inside a slice of HelmAst nodes,
/// ignoring whitespace-only Scalars and HelmComments.
fn lone_if_in(nodes: &[HelmAst]) -> Option<&HelmAst> {
    let mut found: Option<&HelmAst> = None;
    for node in nodes {
        match node {
            HelmAst::Scalar { text } if text.trim().is_empty() => continue,
            HelmAst::HelmComment { .. } => continue,
            HelmAst::If { .. } => {
                if found.is_some() {
                    return None;
                }
                found = Some(node);
            }
            _ => return None,
        }
    }
    found
}

fn scalar_text(node: &HelmAst) -> Option<&str> {
    match node {
        HelmAst::Scalar { text } => Some(text.trim()),
        _ => None,
    }
}

fn dedup_preserve_order(items: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for item in items {
        let trimmed = item.trim().to_string();
        if !trimmed.is_empty() && seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

fn static_literal_outputs(expr: &TemplateExpr) -> Vec<String> {
    let result = eval_expr(expr, &EvalEnv::default());
    if !result.effects.reads.is_empty()
        || !result.effects.output_paths.is_empty()
        || !result.effects.local_source_paths.is_empty()
        || !result.effects.local_rendered_paths.is_empty()
    {
        return Vec::new();
    }
    let strings = result
        .value
        .map(|value| value.strings())
        .unwrap_or_default();
    if strings.len() == 1 {
        strings.into_iter().collect()
    } else {
        Vec::new()
    }
}
