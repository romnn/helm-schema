use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::template_expr_cache::parse_expr_text;
use crate::tree_sitter_utils::children_with_field;
use crate::walker::is_fragment_expr;
use crate::yaml_shape::parse_yaml_key;

fn strip_template_action_wrapping(line: &str) -> Option<String> {
    let after_open = line.trim_start().strip_prefix("{{")?;
    let close_at = after_open.find("}}")?;
    let body = &after_open[..close_at];
    let body = body.strip_prefix('-').unwrap_or(body);
    let body = body.strip_suffix('-').unwrap_or(body);
    Some(body.trim().to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssignmentKind {
    Declaration,
    Assignment,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedHelperAssignment {
    pub(crate) variable: String,
    pub(crate) kind: AssignmentKind,
    pub(crate) rhs: String,
    pub(crate) rhs_expr: TemplateExpr,
}

pub(crate) fn parse_helper_assignment(text: &str) -> Option<ParsedHelperAssignment> {
    let exprs = parse_expr_text(text);
    let [expr] = exprs.as_slice() else {
        return None;
    };
    match expr {
        TemplateExpr::VariableDefinition { name, value } => {
            parsed_assignment_from_expr(text, name, AssignmentKind::Declaration, value)
        }
        TemplateExpr::Assignment { name, value } => {
            parsed_assignment_from_expr(text, name, AssignmentKind::Assignment, value)
        }
        _ => None,
    }
}

fn parsed_assignment_from_expr(
    text: &str,
    name: &str,
    kind: AssignmentKind,
    value: &TemplateExpr,
) -> Option<ParsedHelperAssignment> {
    Some(ParsedHelperAssignment {
        variable: name.trim_start_matches('$').to_string(),
        kind,
        rhs: assignment_rhs_text(text, kind)?,
        rhs_expr: value.clone(),
    })
}

fn assignment_rhs_text(text: &str, kind: AssignmentKind) -> Option<String> {
    let owned;
    let trimmed = if text.trim_start().starts_with("{{") {
        owned = strip_template_action_wrapping(text)?;
        owned.trim()
    } else {
        text.trim()
    };
    let (operator, operator_len) = match kind {
        AssignmentKind::Declaration => (":=", 2usize),
        AssignmentKind::Assignment => ("=", 1usize),
    };
    let index = trimmed.find(operator)?;
    Some(trimmed[index + operator_len..].trim().to_string())
}

pub(crate) fn merge_fragment_locals(
    mut base: HashMap<String, FragmentBinding>,
    other: HashMap<String, FragmentBinding>,
) -> HashMap<String, FragmentBinding> {
    for (key, value) in other {
        let merged = FragmentBinding::union(base.remove(&key), Some(value));
        if let Some(merged) = merged {
            base.insert(key, merged);
        }
    }
    base
}

fn shadow_fragment_binding_keys(
    binding: FragmentBinding,
    keys: BTreeSet<String>,
) -> FragmentBinding {
    if keys.is_empty() {
        return binding;
    }
    let new_entries: BTreeMap<String, FragmentBinding> = keys
        .into_iter()
        .map(|key| (key, FragmentBinding::Unknown))
        .collect();
    match binding {
        FragmentBinding::Overlay {
            mut entries,
            fallback,
        } => {
            entries.extend(new_entries);
            FragmentBinding::Overlay { entries, fallback }
        }
        other => FragmentBinding::Overlay {
            entries: new_entries,
            fallback: Box::new(other),
        },
    }
}

fn local_set_mutation_target_and_keys(
    text: &str,
    local_bindings: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Vec<(String, BTreeSet<String>)> {
    let mut out = Vec::new();
    for expr in parse_expr_text(text) {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            if function != "set" || args.len() < 2 {
                return;
            }
            let TemplateExpr::Variable(var) = &args[0] else {
                return;
            };
            if var.is_empty() || !local_bindings.contains_key(var) {
                return;
            }
            let Some(key_binding) =
                context.fragment_binding_from_expr(&args[1], local_bindings, current_dot, seen)
            else {
                return;
            };
            let keys = FragmentBinding::strings(&key_binding);
            if !keys.is_empty() {
                out.push((var.clone(), keys));
            }
        });
    }
    out
}

pub(crate) fn apply_local_set_mutations(
    text: &str,
    local_bindings: &mut HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> bool {
    let mutations =
        local_set_mutation_target_and_keys(text, local_bindings, current_dot, context, seen);
    let has_mutation = !mutations.is_empty();
    for (var, keys) in mutations {
        if let Some(binding) = local_bindings.remove(&var) {
            local_bindings.insert(var, shadow_fragment_binding_keys(binding, keys));
        }
    }
    has_mutation
}

pub(crate) fn range_variable_item_binding(
    header: &str,
    local_bindings: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<(String, FragmentBinding)> {
    let header = header
        .trim()
        .strip_prefix("range ")
        .unwrap_or_else(|| header.trim());
    let exprs = parse_expr_text(header);
    let [TemplateExpr::VariableDefinition { name, value }] = exprs.as_slice() else {
        return None;
    };
    let binding =
        fragment_binding_from_range_value_expr(value, local_bindings, current_dot, context, seen)?;
    let item = FragmentBinding::item_binding(&binding)?;
    Some((name.trim_start_matches('$').to_string(), item))
}

pub(crate) fn range_variable_name(header: &str) -> Option<String> {
    let header = header
        .trim()
        .strip_prefix("range ")
        .unwrap_or_else(|| header.trim());
    let exprs = parse_expr_text(header);
    let [TemplateExpr::VariableDefinition { name, .. }] = exprs.as_slice() else {
        return None;
    };
    Some(name.trim_start_matches('$').to_string())
}

pub(crate) fn range_iterable_binding(
    header: &str,
    local_bindings: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let header = header
        .trim()
        .strip_prefix("range ")
        .unwrap_or_else(|| header.trim());
    let exprs = parse_expr_text(header);
    let value = match exprs.as_slice() {
        [TemplateExpr::VariableDefinition { value, .. }]
        | [TemplateExpr::Assignment { value, .. }] => value.as_ref(),
        [expr] => expr,
        _ => return None,
    };
    fragment_binding_from_range_value_expr(value, local_bindings, current_dot, context, seen)
}

fn fragment_binding_from_range_value_expr(
    value: &TemplateExpr,
    local_bindings: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    context.fragment_binding_from_expr(value, local_bindings, current_dot, seen)
}

pub(crate) fn range_header_text_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> Option<String> {
    if let Some(range) = node.child_by_field_name("range") {
        return range
            .utf8_text(source.as_bytes())
            .ok()
            .map(|text| text.trim().to_string());
    }
    let mut walker = node.walk();
    for child in node.named_children(&mut walker) {
        if child.kind() == "range_variable_definition"
            && let Some(range) = child.child_by_field_name("range")
        {
            return range
                .utf8_text(source.as_bytes())
                .ok()
                .map(|text| text.trim().to_string());
        }
    }
    None
}

pub(crate) fn range_body_emits_sequence_item_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> bool {
    for body_node in children_with_field(node, "body") {
        let Ok(text) = body_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("- ") || trimmed == "-" {
                return true;
            }
        }
    }
    false
}

pub(crate) fn range_body_renders_scalar_sequence_items_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> bool {
    let mut saw_sequence_item = false;
    let mut body_text = String::new();

    for body_node in children_with_field(node, "body") {
        let Ok(text) = body_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        body_text.push_str(text);
    }

    for line in body_text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix('-') else {
            continue;
        };
        let rest = rest.trim_start();
        saw_sequence_item = true;

        if rest.is_empty() || parse_yaml_key(rest).is_some() || is_fragment_expr(rest) {
            return false;
        }
    }

    saw_sequence_item
}

pub(crate) fn range_has_destructured_variable_definition(node: tree_sitter::Node<'_>) -> bool {
    let mut walker = node.walk();
    node.named_children(&mut walker)
        .find(|child| child.kind() == "range_variable_definition")
        .is_some_and(|definition| {
            let mut definition_walker = definition.walk();
            definition
                .named_children(&mut definition_walker)
                .filter(|child| child.kind() == "variable")
                .count()
                >= 2
        })
}

#[cfg(test)]
mod tests {
    use helm_schema_ast::TemplateExpr;

    use super::{AssignmentKind, parse_helper_assignment};

    #[test]
    fn parse_helper_assignment_detects_declaration_from_ast() {
        let Some(assignment) =
            parse_helper_assignment(r#"{{- $image := .Values.image.repository -}}"#)
        else {
            panic!("parse helper assignment");
        };

        assert_eq!(assignment.variable, "image");
        assert_eq!(assignment.kind, AssignmentKind::Declaration);
        assert_eq!(assignment.rhs, ".Values.image.repository");
        assert_eq!(
            assignment.rhs_expr,
            TemplateExpr::Field(vec![
                "Values".to_string(),
                "image".to_string(),
                "repository".to_string()
            ])
        );
    }

    #[test]
    fn parse_helper_assignment_detects_assignment_from_ast() {
        let Some(assignment) = parse_helper_assignment(r#"{{- $image = .Values.global.image -}}"#)
        else {
            panic!("parse helper assignment");
        };

        assert_eq!(assignment.variable, "image");
        assert_eq!(assignment.kind, AssignmentKind::Assignment);
        assert_eq!(assignment.rhs, ".Values.global.image");
        assert_eq!(
            assignment.rhs_expr,
            TemplateExpr::Field(vec![
                "Values".to_string(),
                "global".to_string(),
                "image".to_string()
            ])
        );
    }
}
