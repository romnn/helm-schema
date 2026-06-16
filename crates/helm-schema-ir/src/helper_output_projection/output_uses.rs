use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::fragment_binding::FragmentBinding;
use crate::helper_binding::HelperBinding;
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::predicate::Predicate;
use crate::template_expr_analysis::expr_contains_helper_call;
use crate::{ValueKind, YamlPath, output_path};

#[derive(Clone, Copy)]
pub(crate) struct HelperOutputExprContext<'a> {
    pub(crate) bindings: &'a HashMap<String, HelperBinding>,
    pub(crate) current_dot: Option<&'a HelperBinding>,
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
    outputs.push(HelperFragmentOutputUse {
        source_expr,
        relative_path: relative_path.clone(),
        kind,
        meta,
    });
}

pub(crate) fn collect_fragment_binding_output_uses(
    outputs: &mut Vec<HelperFragmentOutputUse>,
    binding: &FragmentBinding,
    relative_path: &YamlPath,
    kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    defaulted_paths: &BTreeSet<String>,
) {
    let value = AbstractValue::from_fragment_output_binding(binding);
    collect_abstract_output_uses(
        outputs,
        &value,
        relative_path,
        kind,
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
        collect_abstract_output_uses(
            outputs,
            &value,
            context.relative_path,
            context.kind,
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
    binding: &HelperBinding,
    relative_path: &YamlPath,
    kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    defaulted_paths: &BTreeSet<String>,
) {
    let value = AbstractValue::from_helper_binding(binding);
    collect_abstract_output_uses(
        outputs,
        &value,
        relative_path,
        kind,
        active_output_predicates,
        defaulted_paths,
    );
}

fn collect_abstract_output_uses(
    outputs: &mut Vec<HelperFragmentOutputUse>,
    value: &AbstractValue,
    relative_path: &YamlPath,
    kind: ValueKind,
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
    active_output_predicates: &BTreeSet<Predicate>,
    defaulted_paths: &BTreeSet<String>,
) {
    let base_meta = meta.cloned().unwrap_or_default();
    let meta = helper_output_meta_with_predicates(
        HelperOutputMeta {
            predicates: base_meta.predicates,
            defaulted: base_meta.defaulted || defaulted_paths.contains(path),
        },
        active_output_predicates,
    );
    push_helper_fragment_output(outputs, path.to_string(), relative_path, kind, meta);
}

pub(crate) fn helper_binding_output_meta(
    binding: &HelperBinding,
) -> BTreeMap<String, HelperOutputMeta> {
    AbstractValue::from_helper_binding(binding).output_meta()
}
