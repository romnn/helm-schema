use std::collections::HashSet;

use helm_schema_ast::{DefineIndex, HelmAst, TemplateAction, TemplateExpr};

use crate::capability_branch::{decode_guard, decode_guard_expr};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, literal_helper_call_callee};
use crate::{CapabilityGuard, HelperBranch, HelperBranchBody};

use super::MAX_RECURSION_DEPTH;

pub(crate) struct HelperOutputEvaluator {
    seen: HashSet<String>,
}

#[derive(Clone, Copy)]
enum BodyMode<'a> {
    All,
    Keyed(&'a str),
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
    ) -> Option<HelperBranchBody> {
        Some(self.evaluate_body(std::slice::from_ref(value?), helpers, 0))
    }

    pub(crate) fn evaluate_keyed_inline_branches(
        mut self,
        node: &HelmAst,
        key: &str,
        helpers: &DefineIndex,
    ) -> Option<Vec<HelperBranch>> {
        self.branches_from_if(node, BodyMode::Keyed(key), helpers, 0)
    }

    pub(super) fn evaluate_body(
        &mut self,
        body: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
    ) -> HelperBranchBody {
        self.collect_body(body, BodyMode::All, helpers, depth)
    }

    fn collect_body(
        &mut self,
        body: &[HelmAst],
        mode: BodyMode<'_>,
        helpers: &DefineIndex,
        depth: usize,
    ) -> HelperBranchBody {
        match mode {
            BodyMode::All => self
                .top_level_branches(body, helpers, depth)
                .map(|branches| HelperBranchBody::Nested { branches })
                .unwrap_or_else(|| {
                    HelperBranchBody::literals(dedup_preserve_order(self.collect_literals(
                        body,
                        None,
                        helpers,
                        depth,
                        &mut Vec::new(),
                    )))
                }),
            BodyMode::Keyed(key) => {
                let mut nested = Vec::new();
                let literals = self.collect_literals(body, Some(key), helpers, depth, &mut nested);
                branch_body_from_parts(literals, nested)
            }
        }
    }

    fn top_level_branches(
        &mut self,
        body: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
    ) -> Option<Vec<HelperBranch>> {
        if depth >= MAX_RECURSION_DEPTH {
            return None;
        }
        match single_typed_body(body)? {
            TypedBody::If(node) => {
                let branches = self.branches_from_if(node, BodyMode::All, helpers, depth)?;
                let has_capability_guard = branches.iter().any(|branch| {
                    matches!(
                        branch.guard,
                        Some(CapabilityGuard::Has { .. }) | Some(CapabilityGuard::NotHas { .. })
                    )
                });
                let has_literals = branches.iter().any(|branch| !branch.body.is_empty());
                (has_capability_guard && has_literals).then_some(branches)
            }
            TypedBody::Helper(callee) => self
                .with_helper_body(&callee, helpers, |this, body| {
                    this.top_level_branches(body, helpers, depth + 1)
                })
                .flatten(),
        }
    }

    fn branches_from_if(
        &mut self,
        node: &HelmAst,
        mode: BodyMode<'_>,
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
        if matches!(mode, BodyMode::Keyed(_)) && !capability_guard_is_decoded(&guard) {
            return None;
        }

        let mut branches = vec![HelperBranch {
            guard: Some(guard),
            body: self.collect_body(then_branch, mode, helpers, depth + 1),
        }];
        if let Some(nested_if) = lone_if_in(else_branch) {
            if let Some(nested) = self.branches_from_if(nested_if, mode, helpers, depth + 1) {
                branches.extend(nested);
            }
        } else if !else_branch.is_empty() {
            let body = self.collect_body(else_branch, mode, helpers, depth + 1);
            if !body.is_empty() {
                branches.push(HelperBranch { guard: None, body });
            }
        }
        if matches!(mode, BodyMode::Keyed(_)) {
            branches.retain(|branch| !branch.body.is_empty());
        }
        (!branches.is_empty()).then_some(branches)
    }

    fn collect_literals(
        &mut self,
        nodes: &[HelmAst],
        key_filter: Option<&str>,
        helpers: &DefineIndex,
        depth: usize,
        nested: &mut Vec<HelperBranch>,
    ) -> Vec<String> {
        if depth >= MAX_RECURSION_DEPTH {
            return Vec::new();
        }

        let mut out = Vec::new();
        for node in nodes {
            match node {
                HelmAst::Scalar { text } if key_filter.is_none() => push_nonempty(text, &mut out),
                HelmAst::HelmExpr { action } if key_filter.is_none() => {
                    if let Some(body) = self.evaluate_action_body(action, helpers, depth + 1) {
                        add_body(body, &mut out, nested, false);
                    }
                }
                HelmAst::Pair {
                    key: pair_key,
                    value,
                } if key_filter.is_none() || key_filter == scalar_text(pair_key) => {
                    if let Some(value) = value.as_deref() {
                        let preserve_nested = key_filter.is_some();
                        let body = self.collect_body(
                            std::slice::from_ref(value),
                            BodyMode::All,
                            helpers,
                            depth + 1,
                        );
                        add_body(body, &mut out, nested, preserve_nested);
                    }
                }
                HelmAst::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    if let Some(key) = key_filter {
                        if let Some(branches) =
                            self.branches_from_if(node, BodyMode::Keyed(key), helpers, depth + 1)
                        {
                            nested.extend(branches);
                        }
                    } else {
                        out.extend(self.collect_literals(
                            then_branch,
                            None,
                            helpers,
                            depth + 1,
                            nested,
                        ));
                        out.extend(self.collect_literals(
                            else_branch,
                            None,
                            helpers,
                            depth + 1,
                            nested,
                        ));
                    }
                }
                HelmAst::Range {
                    body, else_branch, ..
                }
                | HelmAst::With {
                    body, else_branch, ..
                } => {
                    out.extend(self.collect_literals(body, key_filter, helpers, depth + 1, nested));
                    out.extend(self.collect_literals(
                        else_branch,
                        key_filter,
                        helpers,
                        depth + 1,
                        nested,
                    ));
                }
                HelmAst::Document { items } | HelmAst::Mapping { items } => {
                    out.extend(self.collect_literals(
                        items,
                        key_filter,
                        helpers,
                        depth + 1,
                        nested,
                    ));
                }
                HelmAst::Sequence { items } if key_filter.is_none() => {
                    out.extend(self.collect_literals(items, None, helpers, depth + 1, nested));
                }
                HelmAst::Define { body, .. } if key_filter.is_none() => {
                    out.extend(self.collect_literals(body, None, helpers, depth + 1, nested));
                }
                HelmAst::Block { body, .. } => {
                    out.extend(self.collect_literals(body, key_filter, helpers, depth + 1, nested));
                }
                HelmAst::Scalar { .. }
                | HelmAst::HelmExpr { .. }
                | HelmAst::Pair { .. }
                | HelmAst::Sequence { .. }
                | HelmAst::Define { .. }
                | HelmAst::HelmComment { .. } => {}
            }
        }
        out
    }

    fn evaluate_action_body(
        &mut self,
        action: &TemplateAction,
        helpers: &DefineIndex,
        depth: usize,
    ) -> Option<HelperBranchBody> {
        let helper_names = helper_call_names(action);
        if !helper_names.is_empty() {
            return combine_branch_bodies(helper_names.into_iter().filter_map(|name| {
                self.with_helper_body(&name, helpers, |this, body| {
                    this.collect_body(body, BodyMode::All, helpers, depth + 1)
                })
            }));
        }

        let literals = dedup_preserve_order(
            action
                .exprs()
                .iter()
                .flat_map(static_literal_outputs)
                .collect(),
        );
        (!literals.is_empty()).then_some(HelperBranchBody::literals(literals))
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

enum TypedBody<'a> {
    If(&'a HelmAst),
    Helper(String),
}

fn single_typed_body(body: &[HelmAst]) -> Option<TypedBody<'_>> {
    let mut out = None;
    for node in body {
        match node {
            HelmAst::Scalar { text } if text.trim().is_empty() => continue,
            HelmAst::HelmComment { .. } => continue,
            HelmAst::If { .. } => {
                if out.is_some() {
                    return None;
                }
                out = Some(TypedBody::If(node));
            }
            HelmAst::HelmExpr { action } => {
                if out.is_some() {
                    return None;
                }
                out = Some(TypedBody::Helper(lone_helper_call_callee(action)?));
            }
            _ => return None,
        }
    }
    out
}

fn capability_guard_is_decoded(guard: &CapabilityGuard) -> bool {
    matches!(
        guard,
        CapabilityGuard::Has { .. } | CapabilityGuard::NotHas { .. }
    )
}

fn branch_body_from_parts(
    literals: Vec<String>,
    mut nested: Vec<HelperBranch>,
) -> HelperBranchBody {
    let literals = dedup_preserve_order(literals);
    if nested.is_empty() {
        return HelperBranchBody::literals(literals);
    }
    if !literals.is_empty() {
        nested.insert(0, HelperBranch::with_literals(None, literals));
    }
    HelperBranchBody::Nested { branches: nested }
}

fn add_body(
    body: HelperBranchBody,
    literals: &mut Vec<String>,
    nested: &mut Vec<HelperBranch>,
    preserve_nested: bool,
) {
    match body {
        HelperBranchBody::Literals { values } => literals.extend(values),
        HelperBranchBody::Nested { branches } if preserve_nested => nested.extend(branches),
        HelperBranchBody::Nested { branches } => {
            let mut seen = HashSet::new();
            for branch in branches {
                branch.body.append_all_literals(literals, &mut seen);
            }
        }
    }
}

fn push_nonempty(text: &str, out: &mut Vec<String>) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
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

fn combine_branch_bodies(
    outputs: impl IntoIterator<Item = HelperBranchBody>,
) -> Option<HelperBranchBody> {
    let mut literals = Vec::new();
    let mut branches = Vec::new();
    for output in outputs {
        add_body(output, &mut literals, &mut branches, true);
    }
    if !branches.is_empty() {
        Some(HelperBranchBody::Nested { branches })
    } else {
        let literals = dedup_preserve_order(literals);
        (!literals.is_empty()).then_some(HelperBranchBody::literals(literals))
    }
}

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

fn lone_if_in(nodes: &[HelmAst]) -> Option<&HelmAst> {
    let mut found = None;
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
    let mut out = Vec::new();
    let mut seen = HashSet::new();
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
