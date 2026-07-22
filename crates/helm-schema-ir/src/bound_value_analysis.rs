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
            .map(|value| {
                let mut segments = helm_schema_core::split_value_path(&binding.base);
                segments.push(value.clone());
                segments.extend(helm_schema_core::split_value_path(&rest));
                helm_schema_core::join_value_path(segments)
            })
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
    let [base, key] = args.as_slice() else {
        return None;
    };
    if function != "get" {
        return None;
    }

    let base = direct_values_path(base.deparen())?;
    let TemplateExpr::Variable(key_var) = key.deparen() else {
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

/// The KEY domain of a literal-dict range expression
/// (`range $k, $v := dict "a" … "b" …`): `$k` iterates exactly the literal
/// keys, so `get map $k` reads decode to that finite member set. The
/// two-variable header surfaces only the range EXPRESSION, but the plain
/// assignment forms are unwrapped too.
pub(crate) fn literal_dict_range_keys(expr: &TemplateExpr) -> Option<Vec<String>> {
    let expr = match expr.deparen() {
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            value.deparen()
        }
        expr => expr,
    };
    literal_dict_keys(expr)
}

fn literal_dict_keys(expr: &TemplateExpr) -> Option<Vec<String>> {
    let TemplateExpr::Call { function, args } = expr else {
        return None;
    };
    if function != "dict" || args.is_empty() || args.len() % 2 != 0 {
        return None;
    }
    let keys = args
        .chunks_exact(2)
        .map(|pair| {
            let [key, _] = pair else {
                return None;
            };
            string_literal_value(key.deparen())
                .filter(|key| !key.is_empty())
                .map(str::to_string)
        })
        .collect::<Option<Vec<_>>>()?;
    (!keys.is_empty()).then_some(keys)
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
    Some((variable.as_str(), helm_schema_core::join_value_path(path)))
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
        TemplateExpr::Call { function, args } if function == "not" => match args.as_slice() {
            [arg] => predicate_domain_constraints(arg, !truthy),
            _ => None,
        },
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

    let constraint = if truthy {
        ValueDomainConstraint {
            allowed: Some(values),
            excluded: BTreeSet::new(),
        }
    } else {
        ValueDomainConstraint {
            allowed: None,
            excluded: values,
        }
    };
    Some(DomainConstraints {
        by_variable: HashMap::from([(variable, constraint)]),
    })
}

#[cfg(test)]
#[path = "tests/bound_value_analysis.rs"]
mod tests;
