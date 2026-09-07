use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::LocalBinding;
use crate::expr_eval::apply_local_set_mutations_expr;
use crate::fragment_expr_eval::FragmentEvalContext;

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

pub(crate) fn parse_helper_assignment_from_exprs(
    exprs: &[TemplateExpr],
) -> Option<ParsedHelperAssignment> {
    let [expr] = exprs else {
        return None;
    };
    match expr {
        TemplateExpr::VariableDefinition { name, value } => Some(parsed_assignment_from_expr(
            name,
            AssignmentKind::Declaration,
            value,
        )),
        TemplateExpr::Assignment { name, value } => Some(parsed_assignment_from_expr(
            name,
            AssignmentKind::Assignment,
            value,
        )),
        _ => None,
    }
}

fn parsed_assignment_from_expr(
    name: &str,
    kind: AssignmentKind,
    value: &TemplateExpr,
) -> ParsedHelperAssignment {
    ParsedHelperAssignment {
        variable: name.trim_start_matches('$').to_string(),
        kind,
        rhs_expr: value.clone(),
    }
}

fn shadow_fragment_value_keys(binding: LocalBinding, keys: BTreeSet<String>) -> LocalBinding {
    if keys.is_empty() {
        return binding;
    }
    let new_entries: BTreeMap<String, AbstractValue> = keys
        .into_iter()
        .map(|key| (key, AbstractValue::Unknown))
        .collect();
    binding.map_values(|value| match value {
        AbstractValue::Overlay {
            mut entries,
            fallback,
        } => {
            entries.extend(new_entries.clone());
            AbstractValue::Overlay { entries, fallback }
        }
        other => AbstractValue::Overlay {
            entries: new_entries.clone(),
            fallback: Box::new(other),
        },
    })
}

fn local_set_mutation_target_and_keys_from_exprs(
    exprs: &[TemplateExpr],
    local_bindings: &HashMap<String, LocalBinding>,
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
            let [target, key, _value] = args.as_slice() else {
                return;
            };
            if function != "set" {
                return;
            }
            let TemplateExpr::Variable(var) = target else {
                return;
            };
            if var.is_empty() || !local_bindings.contains_key(var) {
                return;
            }
            let Some(key_binding) =
                context.fragment_value_from_expr(key, local_bindings, current_dot, seen)
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

pub(crate) fn apply_local_set_mutations_from_exprs(
    exprs: &[TemplateExpr],
    local_bindings: &mut HashMap<String, LocalBinding>,
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
            local_bindings.insert(var, shadow_fragment_value_keys(binding, keys));
        }
    }
    has_mutation
}

fn apply_abstract_local_set_mutations_from_exprs(
    exprs: &[TemplateExpr],
    local_bindings: &mut HashMap<String, LocalBinding>,
    current_dot: Option<&AbstractValue>,
) -> bool {
    let mut env = crate::eval_env::EvalEnv::from_fragment_context(
        local_bindings,
        current_dot,
        crate::eval_env::BindingEvaluationMode::Direct,
    );
    let mut applied = false;
    for expr in exprs {
        applied |= apply_local_set_mutations_expr(expr, &mut env);
    }
    if !applied {
        return false;
    }

    *local_bindings = env
        .locals
        .into_iter()
        .map(|(name, binding)| (name, binding.map_values(|value| value.to_context_value())))
        .collect();
    true
}
