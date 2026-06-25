use std::collections::{BTreeSet, HashSet};

use helm_schema_core::{CapabilityGuard, HelperBranch, HelperBranchBody};

use crate::{DefineIndex, HelmAst, Literal, TemplateAction, TemplateExpr};

const MAX_RECURSION_DEPTH: usize = 6;

pub struct HelperOutputEvaluator {
    seen: HashSet<String>,
}

impl HelperOutputEvaluator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    pub fn evaluate_ast_value(
        mut self,
        value: Option<&HelmAst>,
        helpers: &DefineIndex,
    ) -> Option<HelperBranchBody> {
        Some(self.evaluate_body(std::slice::from_ref(value?), helpers, 0))
    }

    pub fn evaluate_keyed_inline_branches(
        mut self,
        node: &HelmAst,
        key: &str,
        helpers: &DefineIndex,
    ) -> Option<Vec<HelperBranch>> {
        self.branches_from_if(node, Some(key), helpers, 0)
    }

    pub fn evaluate_body(
        &mut self,
        body: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
    ) -> HelperBranchBody {
        self.collect_body(body, None, helpers, depth)
    }

    fn collect_body(
        &mut self,
        body: &[HelmAst],
        key_filter: Option<&str>,
        helpers: &DefineIndex,
        depth: usize,
    ) -> HelperBranchBody {
        if depth >= MAX_RECURSION_DEPTH {
            return HelperBranchBody::literals(Vec::new());
        }
        if key_filter.is_none()
            && let Some(branches) = self.promoted_branches(body, helpers, depth)
        {
            return HelperBranchBody::Nested { branches };
        }

        let mut literals = Vec::new();
        let mut branches = Vec::new();
        for node in body {
            self.collect_node(
                node,
                key_filter,
                helpers,
                depth + 1,
                &mut literals,
                &mut branches,
            );
        }
        body_from_parts(literals, branches)
    }

    fn collect_node(
        &mut self,
        node: &HelmAst,
        key_filter: Option<&str>,
        helpers: &DefineIndex,
        depth: usize,
        literals: &mut Vec<String>,
        branches: &mut Vec<HelperBranch>,
    ) {
        match node {
            HelmAst::Scalar { text } if key_filter.is_none() => push_nonempty(text, literals),
            HelmAst::HelmExpr { action } if key_filter.is_none() => {
                if let Some(body) = self.action_body(action, helpers, depth) {
                    append_body(body, literals, branches, false);
                }
            }
            HelmAst::Pair { key, value }
                if key_filter.is_none() || key_filter == scalar_text(key) =>
            {
                if let Some(value) = value.as_deref() {
                    let body = self.collect_body(std::slice::from_ref(value), None, helpers, depth);
                    append_body(body, literals, branches, key_filter.is_some());
                }
            }
            HelmAst::If {
                then_branch,
                else_branch,
                ..
            } => {
                if let Some(key) = key_filter {
                    if let Some(found) = self.branches_from_if(node, Some(key), helpers, depth) {
                        branches.extend(found);
                    }
                } else {
                    self.collect_nodes(then_branch, None, helpers, depth, literals, branches);
                    self.collect_nodes(else_branch, None, helpers, depth, literals, branches);
                }
            }
            HelmAst::Range {
                body, else_branch, ..
            }
            | HelmAst::With {
                body, else_branch, ..
            } => {
                self.collect_nodes(body, key_filter, helpers, depth, literals, branches);
                self.collect_nodes(else_branch, key_filter, helpers, depth, literals, branches);
            }
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                self.collect_nodes(items, key_filter, helpers, depth, literals, branches);
            }
            HelmAst::Sequence { items } if key_filter.is_none() => {
                self.collect_nodes(items, None, helpers, depth, literals, branches);
            }
            HelmAst::Define { body, .. } if key_filter.is_none() => {
                self.collect_nodes(body, None, helpers, depth, literals, branches);
            }
            HelmAst::Block { body, .. } => {
                self.collect_nodes(body, key_filter, helpers, depth, literals, branches);
            }
            HelmAst::Scalar { .. }
            | HelmAst::HelmExpr { .. }
            | HelmAst::Pair { .. }
            | HelmAst::Sequence { .. }
            | HelmAst::Define { .. }
            | HelmAst::HelmComment { .. } => {}
        }
    }

    fn collect_nodes(
        &mut self,
        nodes: &[HelmAst],
        key_filter: Option<&str>,
        helpers: &DefineIndex,
        depth: usize,
        literals: &mut Vec<String>,
        branches: &mut Vec<HelperBranch>,
    ) {
        for node in nodes {
            self.collect_node(node, key_filter, helpers, depth + 1, literals, branches);
        }
    }

    fn promoted_branches(
        &mut self,
        body: &[HelmAst],
        helpers: &DefineIndex,
        depth: usize,
    ) -> Option<Vec<HelperBranch>> {
        match single_significant_node(body)? {
            HelmAst::If { .. } => {
                let branches =
                    self.branches_from_if(single_significant_node(body)?, None, helpers, depth)?;
                let has_capability_guard = branches.iter().any(|branch| {
                    matches!(
                        branch.guard,
                        Some(CapabilityGuard::Has { .. }) | Some(CapabilityGuard::NotHas { .. })
                    )
                });
                let has_literals = branches.iter().any(|branch| !branch.body.is_empty());
                (has_capability_guard && has_literals).then_some(branches)
            }
            HelmAst::HelmExpr { action } => {
                let callee = pure_helper_call(action)?;
                self.with_helper_body(&callee, helpers, |this, body| {
                    this.promoted_branches(body, helpers, depth + 1)
                })
                .flatten()
            }
            _ => None,
        }
    }

    fn branches_from_if(
        &mut self,
        node: &HelmAst,
        key_filter: Option<&str>,
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

        let guard = crate::decode_guard_expr(condition.expr(), condition.raw())
            .unwrap_or_else(|| crate::decode_guard(condition.raw()));
        if key_filter.is_some() && !guard_is_capability(&guard) {
            return None;
        }

        let mut branches = vec![HelperBranch {
            guard: Some(guard),
            body: self.collect_body(then_branch, key_filter, helpers, depth + 1),
        }];
        if let Some(nested_if) = single_if(else_branch) {
            if let Some(nested) = self.branches_from_if(nested_if, key_filter, helpers, depth + 1) {
                branches.extend(nested);
            }
        } else if !else_branch.is_empty() {
            let body = self.collect_body(else_branch, key_filter, helpers, depth + 1);
            if !body.is_empty() {
                branches.push(HelperBranch { guard: None, body });
            }
        }
        if key_filter.is_some() {
            branches.retain(|branch| !branch.body.is_empty());
        }
        (!branches.is_empty()).then_some(branches)
    }

    fn action_body(
        &mut self,
        action: &TemplateAction,
        helpers: &DefineIndex,
        depth: usize,
    ) -> Option<HelperBranchBody> {
        let helper_names = helper_call_names(action);
        if !helper_names.is_empty() {
            let mut literals = Vec::new();
            let mut branches = Vec::new();
            for name in helper_names {
                if let Some(body) = self.with_helper_body(&name, helpers, |this, body| {
                    this.collect_body(body, None, helpers, depth + 1)
                }) {
                    append_body(body, &mut literals, &mut branches, true);
                }
            }
            return nonempty_body(literals, branches);
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

impl Default for HelperOutputEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

fn body_from_parts(literals: Vec<String>, mut branches: Vec<HelperBranch>) -> HelperBranchBody {
    let literals = dedup_preserve_order(literals);
    if branches.is_empty() {
        return HelperBranchBody::literals(literals);
    }
    if !literals.is_empty() {
        branches.insert(0, HelperBranch::with_literals(None, literals));
    }
    HelperBranchBody::Nested { branches }
}

fn nonempty_body(literals: Vec<String>, branches: Vec<HelperBranch>) -> Option<HelperBranchBody> {
    if !branches.is_empty() {
        Some(HelperBranchBody::Nested { branches })
    } else {
        let literals = dedup_preserve_order(literals);
        (!literals.is_empty()).then_some(HelperBranchBody::literals(literals))
    }
}

fn append_body(
    body: HelperBranchBody,
    literals: &mut Vec<String>,
    branches: &mut Vec<HelperBranch>,
    preserve_nested: bool,
) {
    match body {
        HelperBranchBody::Literals { values } => literals.extend(values),
        HelperBranchBody::Nested { branches: nested } if preserve_nested => {
            branches.extend(nested);
        }
        HelperBranchBody::Nested { branches: nested } => {
            let mut seen = HashSet::new();
            for branch in nested {
                branch.body.append_all_literals(literals, &mut seen);
            }
        }
    }
}

fn single_significant_node(nodes: &[HelmAst]) -> Option<&HelmAst> {
    let mut found = None;
    for node in nodes {
        match node {
            HelmAst::Scalar { text } if text.trim().is_empty() => {}
            HelmAst::HelmComment { .. } => {}
            _ if found.is_none() => found = Some(node),
            _ => return None,
        }
    }
    found
}

fn single_if(nodes: &[HelmAst]) -> Option<&HelmAst> {
    let node = single_significant_node(nodes)?;
    matches!(node, HelmAst::If { .. }).then_some(node)
}

fn scalar_text(node: &HelmAst) -> Option<&str> {
    match node {
        HelmAst::Scalar { text } => Some(text.trim()),
        _ => None,
    }
}

fn push_nonempty(text: &str, out: &mut Vec<String>) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
}

fn guard_is_capability(guard: &CapabilityGuard) -> bool {
    matches!(
        guard,
        CapabilityGuard::Has { .. } | CapabilityGuard::NotHas { .. }
    )
}

fn pure_helper_call(action: &TemplateAction) -> Option<String> {
    let [TemplateExpr::Call { function, args }] = action.exprs() else {
        return None;
    };
    literal_helper_call_callee(function, args).map(str::to_string)
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

fn literal_helper_call_callee<'a>(function: &'a str, args: &'a [TemplateExpr]) -> Option<&'a str> {
    let helper_name = match function {
        "include" | "template" => args.first()?,
        _ => return None,
    };
    match helper_name.deparen() {
        TemplateExpr::Literal(literal) => literal.as_string(),
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
    literal_strings(expr)
        .filter(|strings| strings.len() == 1)
        .map(BTreeSet::into_iter)
        .into_iter()
        .flatten()
        .collect()
}

fn literal_strings(expr: &TemplateExpr) -> Option<BTreeSet<String>> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(BTreeSet::from([value.clone()]))
        }
        TemplateExpr::Call { function, args } => literal_call_strings(function, args),
        TemplateExpr::Pipeline(stages) => literal_pipeline_strings(stages),
        _ => None,
    }
}

fn literal_call_strings(function: &str, args: &[TemplateExpr]) -> Option<BTreeSet<String>> {
    match function {
        "print" => {
            let mut rendered = String::new();
            for arg in args {
                let strings = literal_strings(arg)?;
                let mut values = strings.iter();
                let Some(value) = values.next() else {
                    return None;
                };
                if values.next().is_some() {
                    return None;
                }
                rendered.push_str(value);
            }
            Some(BTreeSet::from([rendered]))
        }
        "printf" => {
            let format = literal_string(args.first()?)?;
            let arg_strings = args
                .iter()
                .skip(1)
                .map(literal_strings)
                .collect::<Option<Vec<_>>>()?;
            crate::render_printf_string_sets(format, &arg_strings)
        }
        "quote" => args.first().and_then(literal_strings),
        _ => None,
    }
}

fn literal_pipeline_strings(stages: &[TemplateExpr]) -> Option<BTreeSet<String>> {
    let (first, rest) = stages.split_first()?;
    let mut current = literal_strings(first)?;
    for stage in rest {
        match stage.deparen() {
            TemplateExpr::Call { function, args } if function == "quote" && args.is_empty() => {}
            _ => return None,
        }
    }
    Some(std::mem::take(&mut current))
}

fn literal_string(expr: &TemplateExpr) -> Option<&str> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => Some(value),
        _ => None,
    }
}
