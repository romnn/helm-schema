use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::expression_analysis::helper_binding_from_expr;
use crate::fragment_binding::FragmentBinding;
use crate::helper_analysis::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::helper_binding::HelperBinding;
use crate::predicate::Predicate;
use crate::template_expr_analysis::expr_contains_helper_call;
use crate::template_expr_cache::parse_expr_text;
use crate::yaml_shape::parse_yaml_key;
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

pub(crate) fn static_yaml_fragment_output_path(text: &str) -> Option<YamlPath> {
    fn printf_format(expr: &TemplateExpr) -> Option<&str> {
        match expr {
            TemplateExpr::Parenthesized(inner) => printf_format(inner),
            TemplateExpr::Call { function, args } if function == "printf" => {
                let TemplateExpr::Literal(Literal::String(format) | Literal::RawString(format)) =
                    args.first()?
                else {
                    return None;
                };
                Some(format)
            }
            TemplateExpr::Pipeline(stages) => stages.first().and_then(printf_format),
            _ => None,
        }
    }

    let exprs = parse_expr_text(text);
    let [expr] = exprs.as_slice() else {
        return None;
    };
    let format = printf_format(expr)?;
    let key = parse_yaml_key(format.trim_start())?.into_key();
    Some(YamlPath(vec![key]))
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
    match binding {
        FragmentBinding::ValuesPath(path) => {
            push_helper_fragment_output(
                outputs,
                path.clone(),
                relative_path,
                kind,
                HelperOutputMeta::with_predicates(
                    active_output_predicates,
                    defaulted_paths.contains(path),
                ),
            );
        }
        FragmentBinding::PathSet(paths) => {
            for path in paths {
                push_helper_fragment_output(
                    outputs,
                    path.clone(),
                    relative_path,
                    kind,
                    HelperOutputMeta::with_predicates(
                        active_output_predicates,
                        defaulted_paths.contains(path),
                    ),
                );
            }
        }
        FragmentBinding::OutputSet(paths) => {
            for path in paths {
                push_helper_fragment_output(
                    outputs,
                    path.clone(),
                    relative_path,
                    kind,
                    HelperOutputMeta::with_predicates(
                        active_output_predicates,
                        defaulted_paths.contains(path),
                    ),
                );
            }
        }
        FragmentBinding::Dict(entries) => {
            for (key, value) in entries {
                let child_path =
                    output_path::append_relative_path(relative_path, &YamlPath(vec![key.clone()]));
                collect_fragment_binding_output_uses(
                    outputs,
                    value,
                    &child_path,
                    value.output_child_kind(),
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        FragmentBinding::Overlay { entries, fallback } => {
            collect_fragment_binding_output_uses(
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
                collect_fragment_binding_output_uses(
                    outputs,
                    value,
                    &child_path,
                    value.output_child_kind(),
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        FragmentBinding::Choice(choices) => {
            for choice in choices {
                collect_fragment_binding_output_uses(
                    outputs,
                    choice,
                    relative_path,
                    kind,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        FragmentBinding::List(items) => {
            let item_path = output_path::sequence_item_path(relative_path);
            for item in items {
                collect_fragment_binding_output_uses(
                    outputs,
                    item,
                    &item_path,
                    item.output_child_kind(),
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        FragmentBinding::ValuesRoot
        | FragmentBinding::RootContext
        | FragmentBinding::Unknown
        | FragmentBinding::StringSet(_) => {}
    }
}

pub(crate) fn collect_helper_binding_output_uses_from_expr(
    expr: &TemplateExpr,
    context: HelperOutputExprContext<'_>,
    outputs: &mut Vec<HelperFragmentOutputUse>,
) {
    if expr_contains_helper_call(expr) {
        return;
    }

    if let Some(binding) =
        helper_binding_from_expr(expr, Some(context.bindings), context.current_dot)
    {
        collect_helper_binding_output_uses(
            outputs,
            &binding,
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
    match binding {
        HelperBinding::ValuesPath(path) => {
            push_helper_fragment_output(
                outputs,
                path.clone(),
                relative_path,
                kind,
                HelperOutputMeta::with_predicates(
                    active_output_predicates,
                    defaulted_paths.contains(path),
                ),
            );
        }
        HelperBinding::PathSet(paths) => {
            for path in paths {
                push_helper_fragment_output(
                    outputs,
                    path.clone(),
                    relative_path,
                    kind,
                    HelperOutputMeta::with_predicates(
                        active_output_predicates,
                        defaulted_paths.contains(path),
                    ),
                );
            }
        }
        HelperBinding::OutputSet(outputs_by_path) => {
            for (path, meta) in outputs_by_path {
                let meta = helper_output_meta_with_predicates(
                    HelperOutputMeta {
                        predicates: meta.predicates.clone(),
                        defaulted: meta.defaulted || defaulted_paths.contains(path),
                    },
                    active_output_predicates,
                );
                push_helper_fragment_output(outputs, path.clone(), relative_path, kind, meta);
            }
        }
        HelperBinding::Dict(entries) => {
            for (key, value) in entries {
                let child_path =
                    output_path::append_relative_path(relative_path, &YamlPath(vec![key.clone()]));
                collect_helper_binding_output_uses(
                    outputs,
                    value,
                    &child_path,
                    value.output_child_kind(),
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        HelperBinding::Overlay { entries, fallback } => {
            collect_helper_binding_output_uses(
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
                collect_helper_binding_output_uses(
                    outputs,
                    value,
                    &child_path,
                    value.output_child_kind(),
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        HelperBinding::Choice(choices) => {
            for choice in choices {
                collect_helper_binding_output_uses(
                    outputs,
                    choice,
                    relative_path,
                    kind,
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        HelperBinding::List(items) => {
            let item_path = output_path::sequence_item_path(relative_path);
            for item in items {
                collect_helper_binding_output_uses(
                    outputs,
                    item,
                    &item_path,
                    item.output_child_kind(),
                    active_output_predicates,
                    defaulted_paths,
                );
            }
        }
        HelperBinding::RootContext | HelperBinding::Unknown | HelperBinding::StringSet(_) => {}
    }
}

pub(crate) fn helper_binding_output_meta(
    binding: &HelperBinding,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out = BTreeMap::new();
    collect_helper_binding_output_meta(binding, &mut out);
    out
}

fn collect_helper_binding_output_meta(
    binding: &HelperBinding,
    out: &mut BTreeMap<String, HelperOutputMeta>,
) {
    match binding {
        HelperBinding::ValuesPath(path) => {
            out.entry(path.clone()).or_default();
        }
        HelperBinding::PathSet(paths) => {
            for path in paths {
                out.entry(path.clone()).or_default();
            }
        }
        HelperBinding::OutputSet(meta_by_path) => {
            for (path, meta) in meta_by_path {
                out.entry(path.clone()).or_default().merge_ref(meta);
            }
        }
        HelperBinding::Dict(entries) => {
            for binding in entries.values() {
                collect_helper_binding_output_meta(binding, out);
            }
        }
        HelperBinding::List(items) => {
            for binding in items {
                collect_helper_binding_output_meta(binding, out);
            }
        }
        HelperBinding::Overlay { entries, fallback } => {
            for binding in entries.values() {
                collect_helper_binding_output_meta(binding, out);
            }
            collect_helper_binding_output_meta(fallback, out);
        }
        HelperBinding::Choice(choices) => {
            for binding in choices {
                collect_helper_binding_output_meta(binding, out);
            }
        }
        HelperBinding::RootContext | HelperBinding::Unknown | HelperBinding::StringSet(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::helper_binding_output_meta;
    use crate::helper_analysis::HelperOutputMeta;
    use crate::helper_binding::HelperBinding;
    use crate::predicate::Predicate;

    #[test]
    fn helper_binding_output_meta_preserves_output_set_metadata() {
        let binding = HelperBinding::Overlay {
            entries: BTreeMap::from([(
                "name".to_string(),
                HelperBinding::ValuesPath("serviceAccount.name".to_string()),
            )]),
            fallback: Box::new(HelperBinding::OutputSet(BTreeMap::from([(
                "global.nameOverride".to_string(),
                HelperOutputMeta {
                    predicates: BTreeSet::from([Predicate::truthy_path(
                        "global.enabled".to_string(),
                    )]),
                    defaulted: true,
                },
            )]))),
        };

        let meta = helper_binding_output_meta(&binding);

        assert!(meta.contains_key("serviceAccount.name"));
        assert_eq!(
            meta.get("global.nameOverride"),
            Some(&HelperOutputMeta {
                predicates: BTreeSet::from([Predicate::truthy_path("global.enabled".to_string())]),
                defaulted: true,
            })
        );
    }
}
