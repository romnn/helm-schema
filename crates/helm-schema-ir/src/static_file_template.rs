use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::{Literal, TemplateExpr, parse_action_expressions};

use crate::binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::template_expr_cache::parse_expr_text;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct StaticFileTemplate {
    pub(crate) path: String,
    pub(crate) dot: Option<FragmentBinding>,
}

#[derive(Clone, Debug)]
pub(crate) struct LiteralHelperCall {
    pub(crate) name: String,
    pub(crate) arg: Option<TemplateExpr>,
}

#[tracing::instrument(skip_all, fields(bytes = text.len()))]
pub(crate) fn literal_helper_calls(text: &str) -> Vec<LiteralHelperCall> {
    let mut out = Vec::new();
    for expr in parse_expr_text(text) {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            if !matches!(function.as_str(), "include" | "template") {
                return;
            }
            let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                return;
            };
            out.push(LiteralHelperCall {
                name: name.clone(),
                arg: args.get(1).cloned(),
            });
        });
    }
    out.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| format!("{:?}", left.arg).cmp(&format!("{:?}", right.arg)))
    });
    out.dedup_by(|left, right| left.name == right.name && left.arg == right.arg);
    out
}

pub(crate) fn collect_template_requests<F>(
    expr: &TemplateExpr,
    resolve_fragment_binding: &mut F,
    requests: &mut BTreeSet<StaticFileTemplate>,
) where
    F: FnMut(&TemplateExpr) -> Option<FragmentBinding>,
{
    if let TemplateExpr::Call { function, args } = expr
        && function == "tpl"
        && let Some(template_arg) = args.first()
    {
        let dot = args.get(1).and_then(&mut *resolve_fragment_binding);
        let mut paths = BTreeSet::new();
        collect_files_get_paths(template_arg, resolve_fragment_binding, &mut paths);
        for path in paths {
            requests.insert(StaticFileTemplate {
                path,
                dot: dot.clone(),
            });
        }
    }

    match expr {
        TemplateExpr::Call { args, .. } => {
            for arg in args {
                collect_template_requests(arg, resolve_fragment_binding, requests);
            }
        }
        TemplateExpr::Selector { operand, .. }
        | TemplateExpr::Parenthesized(operand)
        | TemplateExpr::VariableDefinition { value: operand, .. }
        | TemplateExpr::Assignment { value: operand, .. } => {
            collect_template_requests(operand, resolve_fragment_binding, requests);
        }
        TemplateExpr::Pipeline(stages) => {
            for stage in stages {
                collect_template_requests(stage, resolve_fragment_binding, requests);
            }
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
}

pub(crate) fn collect_template_requests_from_helper(
    name: &str,
    helper_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
) -> BTreeSet<StaticFileTemplate> {
    let Some(source) = context.define_bodies.source(name) else {
        return BTreeSet::new();
    };

    let locals = HashMap::new();
    let mut requests = BTreeSet::new();
    for expr in parse_action_expressions(source) {
        let mut seen = HashSet::new();
        collect_template_requests(
            &expr,
            &mut |expr| context.fragment_binding_from_expr(expr, &locals, helper_dot, &mut seen),
            &mut requests,
        );
    }
    requests
}

fn collect_files_get_paths<F>(
    expr: &TemplateExpr,
    resolve_fragment_binding: &mut F,
    out: &mut BTreeSet<String>,
) where
    F: FnMut(&TemplateExpr) -> Option<FragmentBinding>,
{
    if let TemplateExpr::Call { function, args } = expr
        && is_static_files_get_call(function)
        && let Some(path_arg) = args.first()
        && let Some(binding) = resolve_fragment_binding(path_arg)
    {
        out.extend(FragmentBinding::strings(&binding));
    }

    match expr {
        TemplateExpr::Call { args, .. } => {
            for arg in args {
                collect_files_get_paths(arg, resolve_fragment_binding, out);
            }
        }
        TemplateExpr::Selector { operand, .. }
        | TemplateExpr::Parenthesized(operand)
        | TemplateExpr::VariableDefinition { value: operand, .. }
        | TemplateExpr::Assignment { value: operand, .. } => {
            collect_files_get_paths(operand, resolve_fragment_binding, out);
        }
        TemplateExpr::Pipeline(stages) => {
            for stage in stages {
                collect_files_get_paths(stage, resolve_fragment_binding, out);
            }
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
}

fn is_static_files_get_call(function: &str) -> bool {
    function == "Files.Get" || function == ".Files.Get" || function.ends_with(".Files.Get")
}
