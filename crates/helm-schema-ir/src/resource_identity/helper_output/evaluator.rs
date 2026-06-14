use std::collections::HashSet;

use helm_schema_ast::{DefineIndex, HelmAst, TemplateExpr, parse_action_expressions};

use crate::capability_branch::{CapabilityGuard, HelperBranch, HelperBranchBody, decode_guard};

use super::{HelperOutput, MAX_RECURSION_DEPTH};

pub(super) struct HelperOutputEvaluator {
    seen: HashSet<String>,
}

impl HelperOutputEvaluator {
    pub(super) fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    pub(super) fn evaluate(mut self, name: &str, helpers: &DefineIndex) -> HelperOutput {
        let body = helpers.get(name).unwrap_or(&[]);
        if let Some(branches) = self.extract_top_level_branches(body, helpers, 0) {
            return HelperOutput::Branched { branches };
        }
        let flat = self.collect_literals(body, helpers, 0);
        HelperOutput::Literals(dedup_preserve_order(flat))
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
                HelmAst::HelmExpr { text } => {
                    if if_node.is_some() || lone_helper_call.is_some() {
                        return None;
                    }
                    let Some(callee) = lone_helper_call_callee(text) else {
                        return None;
                    };
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
            cond,
            then_branch,
            else_branch,
        } = if_node
        else {
            unreachable!("if_node is non-None only when matched as If above");
        };
        let mut branches: Vec<HelperBranch> = Vec::new();
        self.collect_if_branches(
            cond,
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

    fn collect_if_branches(
        &mut self,
        cond: &str,
        then_branch: &[HelmAst],
        else_branch: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
        out: &mut Vec<HelperBranch>,
    ) {
        let guard = decode_guard(cond);
        out.push(HelperBranch {
            guard: Some(guard),
            body: self.collect_branch_body(then_branch, helpers, depth + 1),
        });
        // Detect elif-chains: an else-branch consisting solely of an If
        // (plus optional whitespace / comments) is the Helm lowering of
        // `{{ else if ... }}`.
        if let Some(nested_if) = lone_if_in(else_branch) {
            let HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } = nested_if
            else {
                unreachable!("lone_if_in returns only If nodes");
            };
            self.collect_if_branches(cond, then_branch, else_branch, helpers, depth, out);
        } else if !else_branch.is_empty() {
            let body = self.collect_branch_body(else_branch, helpers, depth + 1);
            if !body.is_empty() {
                out.push(HelperBranch { guard: None, body });
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
        if let Some(nested) = self.extract_top_level_branches(nodes, helpers, depth) {
            return HelperBranchBody::Nested { branches: nested };
        }
        let literals = dedup_preserve_order(self.collect_literals(nodes, helpers, depth));
        HelperBranchBody::Literals { values: literals }
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
                HelmAst::HelmExpr { text } => {
                    for value in self.extract_expr_outputs(text, helpers, depth) {
                        out.push(value);
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
                HelmAst::With {
                    body, else_branch, ..
                } => {
                    out.extend(self.collect_literals(body, helpers, depth + 1));
                    out.extend(self.collect_literals(else_branch, helpers, depth + 1));
                }
                HelmAst::Range {
                    body, else_branch, ..
                } => {
                    out.extend(self.collect_literals(body, helpers, depth + 1));
                    out.extend(self.collect_literals(else_branch, helpers, depth + 1));
                }
                HelmAst::Define { body, .. } => {
                    out.extend(self.collect_literals(body, helpers, depth + 1));
                }
                HelmAst::Block { body, .. } => {
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

    fn extract_expr_outputs(
        &mut self,
        text: &str,
        helpers: &DefineIndex,
        depth: usize,
    ) -> Vec<String> {
        // HelmAst::HelmExpr.text is the unwrapped interior of an action.
        // `parse_action_expressions` expects wrapped actions.
        let wrapped = format!("{{{{ {text} }}}}");
        let exprs = parse_action_expressions(&wrapped);
        let mut out: Vec<String> = Vec::new();
        for expr in &exprs {
            match expr.deparen() {
                TemplateExpr::Literal(lit) => {
                    if let Some(value) = lit.as_string() {
                        out.push(value.to_string());
                    }
                }
                TemplateExpr::Call { function, args } => match function.as_str() {
                    "print" | "quote" => {
                        if let Some(value) = single_string_literal_arg(args) {
                            out.push(value);
                        }
                    }
                    "printf" => {
                        if let Some(value) = evaluate_printf(args) {
                            out.push(value);
                        }
                    }
                    "template" | "include" => {
                        let Some(first) = args.first() else {
                            continue;
                        };
                        let TemplateExpr::Literal(lit) = first else {
                            continue;
                        };
                        let Some(name) = lit.as_string() else {
                            continue;
                        };
                        if let Some(values) = self.with_helper_body(name, helpers, |this, body| {
                            this.collect_literals(body, helpers, depth + 1)
                        }) {
                            out.extend(values);
                        }
                    }
                    _ => {}
                },
                TemplateExpr::Pipeline(stages) => {
                    if let Some(last) = stages.last() {
                        match last {
                            TemplateExpr::Literal(lit) => {
                                if let Some(value) = lit.as_string() {
                                    out.push(value.to_string());
                                }
                            }
                            TemplateExpr::Call { function, args } => match function.as_str() {
                                "print" | "quote" => {
                                    if let Some(value) = single_string_literal_arg(args) {
                                        out.push(value);
                                    }
                                }
                                "printf" => {
                                    if let Some(value) = evaluate_printf(args) {
                                        out.push(value);
                                    }
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                    if let Some(seed) = stages.first().and_then(|stage| match stage {
                        TemplateExpr::Literal(lit) => lit.as_string().map(str::to_string),
                        _ => None,
                    }) && stages.iter().skip(1).all(|stage| {
                        matches!(
                            stage,
                            TemplateExpr::Call { function, args }
                                if matches!(function.as_str(), "print" | "quote")
                                    && args.is_empty()
                        )
                    }) {
                        out.push(seed);
                    }
                }
                _ => {}
            }
        }
        out
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

/// If `text` is exactly a `template "X" ...` or `include "X" ...` action
/// (possibly with extra args), return `"X"`. Otherwise `None`.
fn lone_helper_call_callee(text: &str) -> Option<String> {
    let wrapped = format!("{{{{ {text} }}}}");
    let exprs = parse_action_expressions(&wrapped);
    if exprs.len() != 1 {
        return None;
    }
    match &exprs[0] {
        TemplateExpr::Call { function, args }
            if matches!(function.as_str(), "template" | "include") =>
        {
            args.first().and_then(|arg| match arg {
                TemplateExpr::Literal(lit) => lit.as_string().map(str::to_string),
                _ => None,
            })
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

/// Extract the unique string-literal argument from a call's args.
fn single_string_literal_arg(args: &[TemplateExpr]) -> Option<String> {
    if args.len() != 1 {
        return None;
    }
    let TemplateExpr::Literal(lit) = &args[0] else {
        return None;
    };
    lit.as_string().map(str::to_string)
}

/// Statically evaluate `printf` for the exact shapes this evaluator models.
fn evaluate_printf(args: &[TemplateExpr]) -> Option<String> {
    let format = match args.first()? {
        TemplateExpr::Literal(lit) => lit.as_string()?,
        _ => return None,
    };
    if !format.contains('%') {
        if args.len() != 1 {
            return None;
        }
        return Some(format.to_string());
    }
    if format == "%s" && args.len() == 2 {
        let TemplateExpr::Literal(lit) = &args[1] else {
            return None;
        };
        return lit.as_string().map(str::to_string);
    }
    None
}
