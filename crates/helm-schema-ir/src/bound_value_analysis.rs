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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BoundValueContext {
    range_domains: HashMap<String, Vec<String>>,
    get_bindings: HashMap<String, GetBinding>,
    constraints: DomainConstraints,
}

impl BoundValueContext {
    pub(crate) fn new(
        range_domains: &HashMap<String, Vec<String>>,
        get_bindings: &HashMap<String, GetBinding>,
    ) -> Self {
        Self {
            range_domains: range_domains.clone(),
            get_bindings: get_bindings.clone(),
            constraints: DomainConstraints::default(),
        }
    }

    pub(crate) fn selector_paths(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        let Some((variable, rest)) = bound_selector_read(expr) else {
            return BTreeSet::new();
        };
        let Some(binding) = self.get_bindings.get(variable) else {
            return BTreeSet::new();
        };
        let Some(domain) = self.range_domains.get(&binding.key_var) else {
            return BTreeSet::new();
        };

        domain
            .iter()
            .filter(|value| self.constraints.allows(&binding.key_var, value))
            .map(|value| format!("{}.{}.{}", binding.base, value, rest))
            .collect()
    }

    pub(crate) fn with_predicate_constraints(&self, expr: &TemplateExpr, truthy: bool) -> Self {
        let Some(next_constraints) = predicate_domain_constraints(expr, truthy) else {
            return self.clone();
        };
        Self {
            range_domains: self.range_domains.clone(),
            get_bindings: self.get_bindings.clone(),
            constraints: self.constraints.and(&next_constraints),
        }
    }
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

#[cfg(test)]
#[path = "tests/bound_value_analysis.rs"]
mod tests;
