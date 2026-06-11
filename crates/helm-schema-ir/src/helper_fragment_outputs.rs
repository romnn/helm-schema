use std::collections::{BTreeSet, HashMap, HashSet};

use crate::binding::FragmentBinding;
use crate::fragment_expr_eval::{FragmentEvalContext, fragment_binding_from_text};
use crate::fragment_scope_eval::{
    apply_local_set_mutations, merge_fragment_locals, parse_helper_assignment,
    range_body_emits_sequence_item_from_source, range_has_destructured_variable_definition,
    range_header_text_from_source, range_iterable_binding,
};
use crate::tree_sitter_utils::children_with_field;

pub(crate) fn collect_bound_fragment_outputs_from_tree(
    node: tree_sitter::Node<'_>,
    source: &str,
    locals: &mut HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    outputs: &mut BTreeSet<String>,
) {
    match node.kind() {
        "variable_definition" | "assignment" => {
            if let Ok(text) = node.utf8_text(source.as_bytes()) {
                if apply_local_set_mutations(text, locals, current_dot, context, seen) {
                    return;
                }
                if let Some(assignment) = parse_helper_assignment(text) {
                    let binding = context.fragment_binding_from_expr(
                        &assignment.rhs_expr,
                        locals,
                        current_dot,
                        seen,
                    );
                    if let Some(binding) = binding {
                        locals.insert(assignment.variable, binding);
                    }
                }
            }
        }
        "template_action"
        | "dot"
        | "variable"
        | "field"
        | "chained_pipeline"
        | "parenthesized_pipeline"
        | "selector_expression"
        | "function_call"
        | "method_call" => {
            if let Ok(text) = node.utf8_text(source.as_bytes())
                && let Some(binding) =
                    fragment_binding_from_text(text, locals, current_dot, context, seen)
            {
                outputs.extend(FragmentBinding::paths(&binding));
            }
        }
        "if_action" => {
            let mut then_locals = locals.clone();
            for child in children_with_field(node, "consequence") {
                collect_bound_fragment_outputs_from_tree(
                    child,
                    source,
                    &mut then_locals,
                    current_dot,
                    context,
                    seen,
                    outputs,
                );
            }

            let mut else_locals = locals.clone();
            for child in children_with_field(node, "alternative") {
                collect_bound_fragment_outputs_from_tree(
                    child,
                    source,
                    &mut else_locals,
                    current_dot,
                    context,
                    seen,
                    outputs,
                );
            }

            *locals = merge_fragment_locals(then_locals, else_locals);
        }
        "with_action" => {
            let binding = node
                .child_by_field_name("condition")
                .and_then(|condition| condition.utf8_text(source.as_bytes()).ok())
                .and_then(|text| {
                    fragment_binding_from_text(text, locals, current_dot, context, seen)
                });

            let mut body_locals = locals.clone();
            for child in children_with_field(node, "consequence") {
                collect_bound_fragment_outputs_from_tree(
                    child,
                    source,
                    &mut body_locals,
                    binding.as_ref(),
                    context,
                    seen,
                    outputs,
                );
            }

            let mut else_locals = locals.clone();
            for child in children_with_field(node, "alternative") {
                collect_bound_fragment_outputs_from_tree(
                    child,
                    source,
                    &mut else_locals,
                    current_dot,
                    context,
                    seen,
                    outputs,
                );
            }
        }
        "range_action" => {
            let has_destructured_variable_definition =
                range_has_destructured_variable_definition(node);
            let header = range_header_text_from_source(node, source);
            let binding = header
                .as_deref()
                .and_then(|text| range_iterable_binding(text, locals, current_dot, context, seen));
            if has_destructured_variable_definition
                && !range_body_emits_sequence_item_from_source(node, source)
                && let Some(binding) = &binding
            {
                outputs.extend(FragmentBinding::paths(binding));
            }

            let body_dot = binding.as_ref().and_then(FragmentBinding::item_binding);
            let mut body_locals = locals.clone();
            for child in children_with_field(node, "body") {
                collect_bound_fragment_outputs_from_tree(
                    child,
                    source,
                    &mut body_locals,
                    body_dot.as_ref(),
                    context,
                    seen,
                    outputs,
                );
            }
            if binding
                .as_ref()
                .is_some_and(FragmentBinding::definitely_nonempty_iterable)
            {
                *locals = body_locals;
            } else {
                *locals = merge_fragment_locals(locals.clone(), body_locals);
            }
        }
        _ => {
            let mut walker = node.walk();
            for child in node.named_children(&mut walker) {
                collect_bound_fragment_outputs_from_tree(
                    child,
                    source,
                    locals,
                    current_dot,
                    context,
                    seen,
                    outputs,
                );
            }
        }
    }
}
