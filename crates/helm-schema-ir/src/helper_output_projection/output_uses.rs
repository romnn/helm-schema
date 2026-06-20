use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::predicate::Predicate;
use crate::template_expr_analysis::expr_contains_helper_call;
use crate::{ValueKind, YamlPath, output_path};

#[derive(Clone, Copy)]
pub(crate) struct HelperOutputExprContext<'a> {
    pub(crate) bindings: &'a HashMap<String, AbstractValue>,
    pub(crate) current_dot: Option<&'a AbstractValue>,
    pub(crate) relative_path: &'a YamlPath,
    pub(crate) kind: ValueKind,
    pub(crate) active_output_predicates: &'a BTreeSet<Predicate>,
    pub(crate) defaulted_paths: &'a BTreeSet<String>,
}

pub(crate) fn expression_output_use_is_keyed_map_projection(
    output: &HelperFragmentOutputUse,
    expression_base: &YamlPath,
) -> bool {
    let suffix = if output.relative_path.0.starts_with(&expression_base.0) {
        &output.relative_path.0[expression_base.0.len()..]
    } else {
        output.relative_path.0.as_slice()
    };
    !suffix.is_empty() && suffix.iter().all(|segment| !segment.ends_with("[*]"))
}

pub(crate) fn helper_output_meta_with_predicates(
    mut meta: HelperOutputMeta,
    active_output_predicates: &BTreeSet<Predicate>,
) -> HelperOutputMeta {
    meta.add_predicates(active_output_predicates.iter().cloned());
    meta
}

pub(crate) fn push_helper_fragment_output(
    outputs: &mut Vec<HelperFragmentOutputUse>,
    source_expr: String,
    relative_path: &YamlPath,
    kind: ValueKind,
    meta: HelperOutputMeta,
) {
    push_helper_fragment_output_with_encoding(
        outputs,
        source_expr,
        relative_path,
        kind,
        false,
        meta,
    );
}

pub(crate) fn push_helper_fragment_output_with_encoding(
    outputs: &mut Vec<HelperFragmentOutputUse>,
    source_expr: String,
    relative_path: &YamlPath,
    kind: ValueKind,
    encoded: bool,
    meta: HelperOutputMeta,
) {
    outputs.push(HelperFragmentOutputUse {
        source_expr,
        relative_path: relative_path.clone(),
        kind,
        encoded,
        meta,
    });
}

pub(crate) fn collect_fragment_binding_output_uses(
    outputs: &mut Vec<HelperFragmentOutputUse>,
    binding: &AbstractValue,
    relative_path: &YamlPath,
    kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    defaulted_paths: &BTreeSet<String>,
) {
    let value = binding.for_fragment_output_projection();
    let encoded_paths = BTreeSet::new();
    collect_abstract_output_uses(
        outputs,
        &value,
        relative_path,
        kind,
        &encoded_paths,
        active_output_predicates,
        defaulted_paths,
    );
}

pub(crate) fn collect_helper_binding_output_uses_from_expr(
    expr: &TemplateExpr,
    context: HelperOutputExprContext<'_>,
    outputs: &mut Vec<HelperFragmentOutputUse>,
) {
    if expr_contains_helper_call(expr) {
        return;
    }

    let env = EvalEnv::from_helper_context(Some(context.bindings), context.current_dot);
    if let Some(value) = eval_expr(expr, &env).value {
        let encoded_paths = collect_encoded_output_paths_from_expr(expr, &env);
        collect_abstract_output_uses(
            outputs,
            &value,
            context.relative_path,
            context.kind,
            &encoded_paths,
            context.active_output_predicates,
            context.defaulted_paths,
        );
        return;
    }

    match expr {
        TemplateExpr::Call { args, .. } => {
            for arg in args {
                collect_helper_binding_output_uses_from_expr(arg, context, outputs);
            }
        }
        TemplateExpr::Selector { operand, .. } => {
            collect_helper_binding_output_uses_from_expr(operand, context, outputs);
        }
        TemplateExpr::Pipeline(stages) => {
            for stage in stages {
                collect_helper_binding_output_uses_from_expr(stage, context, outputs);
            }
        }
        TemplateExpr::Parenthesized(inner)
        | TemplateExpr::VariableDefinition { value: inner, .. }
        | TemplateExpr::Assignment { value: inner, .. } => {
            collect_helper_binding_output_uses_from_expr(inner, context, outputs);
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
}

pub(crate) fn collect_helper_binding_output_uses(
    outputs: &mut Vec<HelperFragmentOutputUse>,
    binding: &AbstractValue,
    relative_path: &YamlPath,
    kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    defaulted_paths: &BTreeSet<String>,
) {
    let value = binding.clone();
    let encoded_paths = BTreeSet::new();
    collect_abstract_output_uses(
        outputs,
        &value,
        relative_path,
        kind,
        &encoded_paths,
        active_output_predicates,
        defaulted_paths,
    );
}

fn collect_abstract_output_uses(
    outputs: &mut Vec<HelperFragmentOutputUse>,
    value: &AbstractValue,
    relative_path: &YamlPath,
    kind: ValueKind,
    encoded_paths: &BTreeSet<String>,
    active_output_predicates: &BTreeSet<Predicate>,
    defaulted_paths: &BTreeSet<String>,
) {
    match value {
        AbstractValue::ValuesPath(path) => {
            push_output_path(
                outputs,
                path,
                relative_path,
                kind,
                None,
                encoded_paths,
                active_output_predicates,
                defaulted_paths,
            );
        }
        AbstractValue::PathSet(paths) => {
            for path in paths {
                push_output_path(
                    outputs,
                    path,
                    relative_path,
                    kind,
                    None,
                    encoded_paths,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        AbstractValue::OutputSet(outputs_by_path) => {
            for (path, meta) in outputs_by_path {
                push_output_path(
                    outputs,
                    path,
                    relative_path,
                    kind,
                    Some(meta),
                    encoded_paths,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        AbstractValue::Dict(entries) => {
            for (key, value) in entries {
                let child_path =
                    output_path::append_relative_path(relative_path, &YamlPath(vec![key.clone()]));
                collect_abstract_output_uses(
                    outputs,
                    value,
                    &child_path,
                    value.output_child_kind(),
                    encoded_paths,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        AbstractValue::Overlay { entries, fallback } => {
            collect_abstract_output_uses(
                outputs,
                fallback,
                relative_path,
                kind,
                encoded_paths,
                active_output_predicates,
                defaulted_paths,
            );
            for (key, value) in entries {
                let child_path =
                    output_path::append_relative_path(relative_path, &YamlPath(vec![key.clone()]));
                collect_abstract_output_uses(
                    outputs,
                    value,
                    &child_path,
                    value.output_child_kind(),
                    encoded_paths,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        AbstractValue::Choice(choices) => {
            for choice in choices {
                collect_abstract_output_uses(
                    outputs,
                    choice,
                    relative_path,
                    kind,
                    encoded_paths,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        AbstractValue::List(items) => {
            let item_path = output_path::sequence_item_path(relative_path);
            for item in items {
                collect_abstract_output_uses(
                    outputs,
                    item,
                    &item_path,
                    item.output_child_kind(),
                    encoded_paths,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::RootContext
        | AbstractValue::StringSet(_) => {}
    }
}

fn push_output_path(
    outputs: &mut Vec<HelperFragmentOutputUse>,
    path: &str,
    relative_path: &YamlPath,
    kind: ValueKind,
    meta: Option<&HelperOutputMeta>,
    encoded_paths: &BTreeSet<String>,
    active_output_predicates: &BTreeSet<Predicate>,
    defaulted_paths: &BTreeSet<String>,
) {
    let base_meta = meta.cloned().unwrap_or_default();
    let meta = helper_output_meta_with_predicates(
        HelperOutputMeta {
            predicates: base_meta.predicates,
            defaulted: base_meta.defaulted || defaulted_paths.contains(path),
            provenance: base_meta.provenance,
        },
        active_output_predicates,
    );
    push_helper_fragment_output_with_encoding(
        outputs,
        path.to_string(),
        relative_path,
        kind,
        path_is_encoded(path, encoded_paths),
        meta,
    );
}

fn collect_encoded_output_paths_from_expr(expr: &TemplateExpr, env: &EvalEnv) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    append_encoded_output_paths_from_expr(expr, env, &mut out);
    out
}

fn append_encoded_output_paths_from_expr(
    expr: &TemplateExpr,
    env: &EvalEnv,
    out: &mut BTreeSet<String>,
) {
    match expr.deparen() {
        TemplateExpr::Call { function, args } => {
            if function == "b64enc" {
                for arg in args {
                    append_expr_value_paths(arg, env, out);
                }
            }
            for arg in args {
                append_encoded_output_paths_from_expr(arg, env, out);
            }
        }
        TemplateExpr::Pipeline(stages) => {
            append_pipeline_encoded_output_paths(stages, env, out);
        }
        TemplateExpr::Selector { operand, .. } | TemplateExpr::Parenthesized(operand) => {
            append_encoded_output_paths_from_expr(operand, env, out);
        }
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            append_encoded_output_paths_from_expr(value, env, out);
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
}

fn append_pipeline_encoded_output_paths(
    stages: &[TemplateExpr],
    env: &EvalEnv,
    out: &mut BTreeSet<String>,
) {
    let mut prefix: Vec<TemplateExpr> = Vec::new();
    for stage in stages {
        let current = stage.deparen();
        if let TemplateExpr::Call { function, args } = current {
            for arg in args {
                append_encoded_output_paths_from_expr(arg, env, out);
            }
            if function == "b64enc" {
                if !prefix.is_empty() {
                    let prefix_expr = if prefix.len() == 1 {
                        prefix[0].clone()
                    } else {
                        TemplateExpr::Pipeline(prefix.clone())
                    };
                    append_expr_value_paths(&prefix_expr, env, out);
                }
                for arg in args {
                    append_expr_value_paths(arg, env, out);
                }
            }
        } else {
            append_encoded_output_paths_from_expr(current, env, out);
        }
        prefix.push(stage.clone());
    }
}

fn append_expr_value_paths(expr: &TemplateExpr, env: &EvalEnv, out: &mut BTreeSet<String>) {
    if let Some(value) = eval_expr(expr, env).value {
        out.extend(value.paths().into_iter().filter(|path| !path.is_empty()));
    }
}

fn path_is_encoded(path: &str, encoded_paths: &BTreeSet<String>) -> bool {
    encoded_paths.iter().any(|encoded_path| {
        path == encoded_path
            || path
                .strip_prefix(encoded_path)
                .is_some_and(|suffix| suffix.starts_with('.'))
    })
}

pub(crate) fn helper_binding_output_meta(
    binding: &AbstractValue,
) -> BTreeMap<String, HelperOutputMeta> {
    binding.clone().output_meta()
}
