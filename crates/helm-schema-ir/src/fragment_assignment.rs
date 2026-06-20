use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::expr_eval::apply_local_set_mutations_expr;
use crate::fragment_expr_eval::FragmentEvalContext;

#[cfg(test)]
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
    pub(crate) rhs_expr: TemplateExpr,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedHelperAssignmentWithRhs {
    pub(crate) variable: String,
    pub(crate) kind: AssignmentKind,
    pub(crate) rhs: String,
    pub(crate) rhs_expr: TemplateExpr,
}

#[cfg(test)]
pub(crate) fn parse_helper_assignment(text: &str) -> Option<ParsedHelperAssignmentWithRhs> {
    use crate::template_expr_cache::parse_expr_text;
    let assignment = parse_helper_assignment_from_exprs(&parse_expr_text(text))?;
    Some(ParsedHelperAssignmentWithRhs {
        variable: assignment.variable,
        kind: assignment.kind,
        rhs: assignment_rhs_text(text, assignment.kind)?,
        rhs_expr: assignment.rhs_expr,
    })
}

pub(crate) fn parse_helper_assignment_from_exprs(
    exprs: &[TemplateExpr],
) -> Option<ParsedHelperAssignment> {
    let [expr] = exprs else {
        return None;
    };
    match expr {
        TemplateExpr::VariableDefinition { name, value } => {
            parsed_assignment_from_expr(name, AssignmentKind::Declaration, value)
        }
        TemplateExpr::Assignment { name, value } => {
            parsed_assignment_from_expr(name, AssignmentKind::Assignment, value)
        }
        _ => None,
    }
}

fn parsed_assignment_from_expr(
    name: &str,
    kind: AssignmentKind,
    value: &TemplateExpr,
) -> Option<ParsedHelperAssignment> {
    Some(ParsedHelperAssignment {
        variable: name.trim_start_matches('$').to_string(),
        kind,
        rhs_expr: value.clone(),
    })
}

#[cfg(test)]
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
    mut base: HashMap<String, AbstractValue>,
    other: HashMap<String, AbstractValue>,
) -> HashMap<String, AbstractValue> {
    for (key, value) in other {
        let merged = match base.remove(&key) {
            Some(existing) => AbstractValue::choice(vec![existing, value]),
            None => Some(value),
        };
        if let Some(merged) = merged {
            base.insert(key, merged);
        }
    }
    base
}

fn shadow_fragment_binding_keys(binding: AbstractValue, keys: BTreeSet<String>) -> AbstractValue {
    if keys.is_empty() {
        return binding;
    }
    let new_entries: BTreeMap<String, AbstractValue> = keys
        .into_iter()
        .map(|key| (key, AbstractValue::Unknown))
        .collect();
    match binding {
        AbstractValue::Overlay {
            mut entries,
            fallback,
        } => {
            entries.extend(new_entries);
            AbstractValue::Overlay { entries, fallback }
        }
        other => AbstractValue::Overlay {
            entries: new_entries,
            fallback: Box::new(other),
        },
    }
}

fn local_set_mutation_target_and_keys_from_exprs(
    exprs: &[TemplateExpr],
    local_bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Vec<(String, BTreeSet<String>)> {
    let mut out = Vec::new();
    for expr in exprs {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            if function != "set" || args.len() != 3 {
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
            let keys = key_binding.strings();
            if !keys.is_empty() {
                out.push((var.clone(), keys));
            }
        });
    }
    out
}

#[cfg(test)]
pub(crate) fn apply_local_set_mutations(
    text: &str,
    local_bindings: &mut HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> bool {
    use crate::template_expr_cache::parse_expr_text;
    apply_local_set_mutations_from_exprs(
        &parse_expr_text(text),
        local_bindings,
        current_dot,
        context,
        seen,
    )
}

pub(crate) fn apply_local_set_mutations_from_exprs(
    exprs: &[TemplateExpr],
    local_bindings: &mut HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> bool {
    let abstract_applied =
        apply_abstract_local_set_mutations_from_exprs(exprs, local_bindings, current_dot);
    if abstract_applied {
        return true;
    }

    let mutations = local_set_mutation_target_and_keys_from_exprs(
        exprs,
        local_bindings,
        current_dot,
        context,
        seen,
    );
    let has_mutation = !mutations.is_empty();
    for (var, keys) in mutations {
        if let Some(binding) = local_bindings.remove(&var) {
            local_bindings.insert(var, shadow_fragment_binding_keys(binding, keys));
        }
    }
    has_mutation
}

fn apply_abstract_local_set_mutations_from_exprs(
    exprs: &[TemplateExpr],
    local_bindings: &mut HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
) -> bool {
    let mut env = crate::eval_env::EvalEnv::from_fragment_context(local_bindings, current_dot);
    let mut applied = false;
    for expr in exprs {
        applied |= apply_local_set_mutations_expr(&expr, &mut env);
    }
    if !applied {
        return false;
    }

    let mut converted = HashMap::new();
    for (name, value) in env.locals {
        converted.insert(name, value.to_context_value());
    }
    *local_bindings = converted;
    true
}
