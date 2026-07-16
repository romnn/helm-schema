use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::expr_eval::literal_helper_call_callee;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_meta::HelperOutputMeta;
use crate::node_eval::{NodeAction, node_action};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StaticTemplateSource {
    File { path: String },
    ValuesDefault { path: String, program: String },
    Constructed { program: String },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct StaticTemplateProgram {
    pub(crate) source: StaticTemplateSource,
    pub(crate) dot: Option<AbstractValue>,
}

#[derive(Clone, Debug)]
pub(crate) struct LiteralHelperCall {
    pub(crate) name: String,
    pub(crate) arg: Option<TemplateExpr>,
}

pub(crate) fn literal_helper_calls_from_exprs(exprs: &[TemplateExpr]) -> Vec<LiteralHelperCall> {
    let mut out = Vec::new();
    for expr in exprs {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            let Some(name) = literal_helper_call_callee(function, args) else {
                return;
            };
            out.push(LiteralHelperCall {
                name: name.to_string(),
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

pub(crate) fn collect_template_requests_from_helper(
    name: &str,
    helper_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
) -> BTreeSet<StaticTemplateProgram> {
    let Some(body) = context.analysis_db.parsed_helper_body(name) else {
        return BTreeSet::new();
    };

    let locals = HashMap::new();
    let local_output_meta = HashMap::new();
    let mut requests = BTreeSet::new();
    walk_template_exprs(body.source, body.tree.root_node(), &mut |expr| {
        requests.extend(collect_template_requests_from_exprs(
            std::slice::from_ref(expr),
            helper_dot,
            &locals,
            &local_output_meta,
            context,
        ));
    });
    requests
}

pub(crate) fn collect_template_requests_from_exprs(
    exprs: &[TemplateExpr],
    current_dot: Option<&AbstractValue>,
    locals: &HashMap<String, AbstractValue>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'_>,
) -> BTreeSet<StaticTemplateProgram> {
    let mut requests = BTreeSet::new();
    for expr in exprs {
        let mut seen = HashSet::new();
        let mut resolve_fragment_value = |expr: &TemplateExpr| {
            context.fragment_value_from_expr_with_meta(
                expr,
                locals,
                local_output_meta,
                current_dot,
                &mut seen,
            )
        };
        expr.walk(|node| {
            if let TemplateExpr::Call { function, args } = node
                && function == "tpl"
                && let Some(template_arg) = args.first()
            {
                let dot = args.get(1).and_then(&mut resolve_fragment_value);
                let mut paths = BTreeSet::new();
                collect_files_get_paths(template_arg, &mut resolve_fragment_value, &mut paths);
                for path in paths {
                    requests.insert(StaticTemplateProgram {
                        source: StaticTemplateSource::File { path },
                        dot: dot.clone(),
                    });
                }
                if let Some(value) = resolve_fragment_value(template_arg) {
                    for path in value.fragment_source_paths() {
                        let Some(program) = context.analysis_db.chart_default_string(&path) else {
                            continue;
                        };
                        requests.insert(StaticTemplateProgram {
                            source: StaticTemplateSource::ValuesDefault {
                                path,
                                program: program.to_string(),
                            },
                            dot: dot.clone(),
                        });
                    }
                    for program in value.strings() {
                        if !matches!(
                            helm_schema_ast::contains_template_action(&program),
                            Ok(true)
                        ) {
                            continue;
                        }
                        requests.insert(StaticTemplateProgram {
                            source: StaticTemplateSource::Constructed { program },
                            dot: dot.clone(),
                        });
                    }
                }
            }
        });
    }
    requests
}

fn walk_template_exprs(
    source: &str,
    node: tree_sitter::Node<'_>,
    visit: &mut impl FnMut(&TemplateExpr),
) {
    match node_action(source, node) {
        NodeAction::Assignment(Some(exprs)) | NodeAction::Output(Some(exprs)) => {
            for expr in &exprs {
                visit(expr);
            }
        }
        NodeAction::If(Some(header))
        | NodeAction::With(Some(header))
        | NodeAction::Range(Some(header)) => visit(header.expr()),
        NodeAction::Text
        | NodeAction::Suppressed
        | NodeAction::Assignment(None)
        | NodeAction::If(None)
        | NodeAction::With(None)
        | NodeAction::Range(None)
        | NodeAction::Output(None)
        | NodeAction::Descend => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_template_exprs(source, child, visit);
    }
}

fn collect_files_get_paths<F>(
    expr: &TemplateExpr,
    resolve_fragment_value: &mut F,
    out: &mut BTreeSet<String>,
) where
    F: FnMut(&TemplateExpr) -> Option<AbstractValue>,
{
    expr.walk(|node| {
        if let TemplateExpr::Call { function, args } = node
            && is_static_files_get_call(function)
            && let Some(path_arg) = args.first()
            && let Some(binding) = resolve_fragment_value(path_arg)
        {
            out.extend(binding.strings());
        }
    });
}

fn is_static_files_get_call(function: &str) -> bool {
    function == "Files.Get" || function.ends_with(".Files.Get")
}
