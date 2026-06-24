use std::collections::{BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::expr_eval::direct_values_path;
use crate::fragment_assignment::AssignmentKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GetBinding {
    pub(crate) base: String,
    pub(crate) key_var: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GetBindingPlan {
    pub(crate) variable: String,
    pub(crate) kind: AssignmentKind,
    pub(crate) binding: GetBinding,
}

pub(crate) fn parse_literal_list_range_expr(expr: &TemplateExpr) -> Option<(String, Vec<String>)> {
    let TemplateExpr::VariableDefinition { name, value } = expr.deparen() else {
        return None;
    };
    let variable = name.trim_start_matches('$');
    if variable.is_empty() {
        return None;
    }
    let values = literal_list_values(value.deparen())?;
    Some((variable.to_string(), values))
}

pub(crate) fn parse_get_binding_from_exprs(exprs: &[TemplateExpr]) -> Option<GetBindingPlan> {
    let [expr] = exprs else {
        return None;
    };
    match expr {
        TemplateExpr::VariableDefinition { name, value } => {
            get_binding_plan_from_expr(name, AssignmentKind::Declaration, value.deparen())
        }
        TemplateExpr::Assignment { name, value } => {
            get_binding_plan_from_expr(name, AssignmentKind::Assignment, value.deparen())
        }
        _ => None,
    }
}

fn get_binding_plan_from_expr(
    variable: &str,
    kind: AssignmentKind,
    expr: &TemplateExpr,
) -> Option<GetBindingPlan> {
    let TemplateExpr::Call { function, args } = expr else {
        return None;
    };
    if function != "get" || args.len() != 2 {
        return None;
    }

    let base = direct_values_path(args[0].deparen())?;
    let TemplateExpr::Variable(key_var) = args[1].deparen() else {
        return None;
    };
    if key_var.is_empty() {
        return None;
    }
    Some(GetBindingPlan {
        variable: variable.trim_start_matches('$').to_string(),
        kind,
        binding: GetBinding {
            base,
            key_var: key_var.clone(),
        },
    })
}

fn literal_list_values(expr: &TemplateExpr) -> Option<Vec<String>> {
    let TemplateExpr::Call { function, args } = expr else {
        return None;
    };
    if function != "list" && function != "tuple" {
        return None;
    }

    let values = args
        .iter()
        .map(|arg| string_literal_value(arg.deparen()).filter(|value| !value.is_empty()))
        .map(|value| value.map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    (!values.is_empty()).then_some(values)
}

fn string_literal_value(expr: &TemplateExpr) -> Option<&str> {
    match expr {
        TemplateExpr::Literal(literal) => literal.as_string(),
        _ => None,
    }
}

fn bound_selector_read(expr: &TemplateExpr) -> Option<(&str, String)> {
    let TemplateExpr::Selector { operand, path } = expr else {
        return None;
    };
    let TemplateExpr::Variable(variable) = operand.deparen() else {
        return None;
    };
    if variable.is_empty() || path.is_empty() {
        return None;
    }
    Some((variable.as_str(), path.join(".")))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct DomainConstraints {
    by_variable: HashMap<String, ValueDomainConstraint>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ValueDomainConstraint {
    allowed: Option<BTreeSet<String>>,
    excluded: BTreeSet<String>,
}

impl DomainConstraints {
    fn and(&self, other: &Self) -> Self {
        let mut combined = self.clone();
        for (variable, constraint) in &other.by_variable {
            combined
                .by_variable
                .entry(variable.clone())
                .or_default()
                .intersect_with(constraint);
        }
        combined
    }

    fn allows(&self, variable: &str, value: &str) -> bool {
        self.by_variable
            .get(variable)
            .is_none_or(|constraint| constraint.allows(value))
    }

    fn require_one_of(variable: &str, values: BTreeSet<String>) -> Self {
        Self {
            by_variable: HashMap::from([(
                variable.to_string(),
                ValueDomainConstraint {
                    allowed: Some(values),
                    excluded: BTreeSet::new(),
                },
            )]),
        }
    }

    fn exclude(variable: &str, values: BTreeSet<String>) -> Self {
        Self {
            by_variable: HashMap::from([(
                variable.to_string(),
                ValueDomainConstraint {
                    allowed: None,
                    excluded: values,
                },
            )]),
        }
    }
}

impl ValueDomainConstraint {
    fn intersect_with(&mut self, other: &Self) {
        self.allowed = match (&self.allowed, &other.allowed) {
            (Some(left), Some(right)) => Some(left.intersection(right).cloned().collect()),
            (Some(left), None) => Some(left.clone()),
            (None, Some(right)) => Some(right.clone()),
            (None, None) => None,
        };
        self.excluded.extend(other.excluded.iter().cloned());
        if let Some(allowed) = &mut self.allowed {
            allowed.retain(|value| !self.excluded.contains(value));
        }
    }

    fn allows(&self, value: &str) -> bool {
        if self.excluded.contains(value) {
            return false;
        }
        self.allowed
            .as_ref()
            .is_none_or(|allowed| allowed.contains(value))
    }
}

fn predicate_domain_constraints(expr: &TemplateExpr, truthy: bool) -> Option<DomainConstraints> {
    match expr.deparen() {
        TemplateExpr::Call { function, args } if function == "not" && args.len() == 1 => {
            predicate_domain_constraints(&args[0], !truthy)
        }
        TemplateExpr::Call { function, args } if function == "eq" => {
            eq_domain_constraints(args, truthy)
        }
        TemplateExpr::Call { function, args } if function == "ne" && args.len() == 2 => {
            eq_domain_constraints(args, !truthy)
        }
        _ => None,
    }
}

fn eq_domain_constraints(args: &[TemplateExpr], truthy: bool) -> Option<DomainConstraints> {
    let variables: BTreeSet<String> = args
        .iter()
        .filter_map(|arg| match arg.deparen() {
            TemplateExpr::Variable(variable) if !variable.is_empty() => Some(variable.clone()),
            _ => None,
        })
        .collect();
    let values: BTreeSet<String> = args
        .iter()
        .filter_map(|arg| string_literal_value(arg.deparen()).map(str::to_string))
        .filter(|value| !value.is_empty())
        .collect();

    let mut variables = variables.into_iter();
    let variable = variables.next()?;
    if variables.next().is_some() || values.is_empty() {
        return None;
    }

    Some(if truthy {
        DomainConstraints::require_one_of(&variable, values)
    } else {
        DomainConstraints::exclude(&variable, values)
    })
}

pub(crate) fn extract_bound_values_from_exprs(
    exprs: &[TemplateExpr],
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();

    for expr in exprs {
        collect_bound_values_from_expr(
            expr,
            &DomainConstraints::default(),
            range_domains,
            get_bindings,
            &mut out,
        );
    }

    out.into_iter().collect()
}

pub(crate) fn extract_bound_values_expr(
    expr: &TemplateExpr,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> Vec<String> {
    extract_bound_values_from_exprs(std::slice::from_ref(expr), range_domains, get_bindings)
}

fn collect_bound_values_from_expr(
    expr: &TemplateExpr,
    constraints: &DomainConstraints,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
    out: &mut BTreeSet<String>,
) {
    match expr.deparen() {
        TemplateExpr::Call { function, args } if function == "and" => {
            collect_short_circuit_args(args, true, constraints, range_domains, get_bindings, out);
        }
        TemplateExpr::Call { function, args } if function == "or" => {
            collect_short_circuit_args(args, false, constraints, range_domains, get_bindings, out);
        }
        TemplateExpr::Call { args, .. } => {
            for arg in args {
                collect_bound_values_from_expr(arg, constraints, range_domains, get_bindings, out);
            }
        }
        TemplateExpr::Pipeline(stages) => {
            for stage in stages {
                collect_bound_values_from_expr(
                    stage,
                    constraints,
                    range_domains,
                    get_bindings,
                    out,
                );
            }
        }
        TemplateExpr::Selector { operand, .. } => {
            collect_bound_selector_value(
                expr.deparen(),
                constraints,
                range_domains,
                get_bindings,
                out,
            );
            collect_bound_values_from_expr(operand, constraints, range_domains, get_bindings, out);
        }
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            collect_bound_values_from_expr(value, constraints, range_domains, get_bindings, out);
        }
        TemplateExpr::Parenthesized(_)
        | TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
}

fn collect_short_circuit_args(
    args: &[TemplateExpr],
    previous_truthy: bool,
    constraints: &DomainConstraints,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
    out: &mut BTreeSet<String>,
) {
    let mut arg_constraints = constraints.clone();
    for arg in args {
        collect_bound_values_from_expr(arg, &arg_constraints, range_domains, get_bindings, out);
        if let Some(next_constraints) = predicate_domain_constraints(arg, previous_truthy) {
            arg_constraints = arg_constraints.and(&next_constraints);
        }
    }
}

fn collect_bound_selector_value(
    expr: &TemplateExpr,
    constraints: &DomainConstraints,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
    out: &mut BTreeSet<String>,
) {
    let Some((variable, rest)) = bound_selector_read(expr) else {
        return;
    };
    let Some(binding) = get_bindings.get(variable) else {
        return;
    };
    let Some(domain) = range_domains.get(&binding.key_var) else {
        return;
    };

    for value in domain {
        if constraints.allows(&binding.key_var, value) {
            out.insert(format!("{}.{}.{}", binding.base, value, rest));
        }
    }
}

#[cfg(test)]
#[path = "tests/bound_value_analysis.rs"]
mod tests;
