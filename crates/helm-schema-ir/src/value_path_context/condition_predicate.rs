use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::{Guard, GuardValue};
use helm_schema_ast::type_is_schema_type;
use helm_schema_core::Predicate;

use super::ValuePathContext;

impl ValuePathContext<'_> {
    pub(crate) fn condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        if let Some(predicate) = self.condition_predicate(expr) {
            return predicate;
        }
        if self.condition_has_unrepresentable_values_comparison_expr(expr) {
            return Predicate::True;
        }
        self.truthy_predicate(expr).unwrap_or(Predicate::True)
    }

    pub(crate) fn with_condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        Predicate::all(
            self.condition_predicate_expr(expr)
                .with_context_predicates(),
        )
    }

    fn condition_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return self.truthy_predicate(expr);
        };
        match function.as_str() {
            "and" => self.and_predicate(args),
            "not" => self.not_predicate(args),
            "empty" => self.empty_predicate(args),
            "or" => self.or_predicate(args),
            "eq" => self.value_comparison_predicate(args, false),
            "ne" => self.value_comparison_predicate(args, true),
            "typeIs" => self.type_is_predicate(args),
            _ => self.truthy_predicate(expr),
        }
    }

    fn and_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let predicates = args
            .iter()
            .filter_map(|arg| self.condition_predicate(arg))
            .collect::<Vec<_>>();
        (!predicates.is_empty()).then(|| Predicate::all(predicates))
    }

    fn not_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [arg] = args else {
            return None;
        };

        match arg.deparen() {
            TemplateExpr::Call { function, args } if function == "empty" => self
                .empty_predicate(args)
                .map(|predicate| predicate.negated()),
            TemplateExpr::Call { function, args } if function == "or" => {
                self.negated_or_predicate(args)
            }
            TemplateExpr::Call { function, args } if function == "eq" => {
                self.value_comparison_predicate(args, true)
            }
            TemplateExpr::Call { function, args } if function == "ne" => {
                self.value_comparison_predicate(args, false)
            }
            _ => {
                let paths = self.paths_for_expr(arg);
                if paths.len() == 1 {
                    return paths
                        .into_iter()
                        .next()
                        .map(|path| Predicate::truthy_path(path).negated());
                }
                None
            }
        }
    }

    fn negated_or_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let predicates = args
            .iter()
            .map(|arg| {
                self.single_truthy_predicate(arg)
                    .map(|predicate| predicate.negated())
            })
            .collect::<Option<Vec<_>>>()?;
        (!predicates.is_empty()).then(|| Predicate::all(predicates))
    }

    fn empty_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [arg] = args else {
            return None;
        };
        self.single_truthy_predicate(arg)
            .map(|predicate| predicate.negated())
    }

    fn or_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let mut truthy_paths = BTreeSet::new();
        let mut alternatives = Vec::new();
        for arg in args {
            let paths = self.paths_for_expr(arg);
            if !paths.is_empty() && !matches!(arg.deparen(), TemplateExpr::Call { .. }) {
                truthy_paths.extend(paths);
                continue;
            }
            alternatives.push(self.condition_predicate(arg)?);
        }
        if !truthy_paths.is_empty() {
            let predicate = Predicate::Or(
                truthy_paths
                    .into_iter()
                    .map(Predicate::truthy_path)
                    .collect(),
            );
            alternatives.push(predicate);
        }
        (!alternatives.is_empty()).then_some(Predicate::Or(alternatives))
    }

    fn type_is_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let schema_type = type_is_schema_type(args.first())?;
        let predicates = args
            .iter()
            .skip(1)
            .flat_map(|arg| self.paths_for_expr(arg))
            .map(|path| {
                Predicate::from(Guard::TypeIs {
                    path,
                    schema_type: schema_type.clone(),
                })
            })
            .collect::<Vec<_>>();
        (!predicates.is_empty()).then(|| Predicate::all(predicates))
    }

    fn truthy_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        let paths = self.paths_for_expr(expr);
        (!paths.is_empty())
            .then(|| Predicate::all(paths.into_iter().map(Predicate::truthy_path).collect()))
    }

    fn single_truthy_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        let mut paths = self.paths_for_expr(expr).into_iter();
        let path = paths.next()?;
        paths.next().is_none().then(|| Predicate::truthy_path(path))
    }

    fn condition_has_unrepresentable_values_comparison_expr(&self, expr: &TemplateExpr) -> bool {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return false;
        };
        match function.as_str() {
            "eq" | "ne" => self.comparison_has_unrepresentable_values(args),
            "typeIs" => {
                args.iter()
                    .any(|arg| self.expr_needs_context_value_resolution(arg))
                    && self.type_is_predicate(args).is_none()
            }
            _ => false,
        }
    }

    fn value_comparison_predicate(
        &self,
        args: &[TemplateExpr],
        negated: bool,
    ) -> Option<Predicate> {
        let [left, right] = args else {
            return None;
        };
        let (value, paths) = match (guard_value_literal(left), guard_value_literal(right)) {
            (Some(value), None) => (value, self.paths_for_expr(right)),
            (None, Some(value)) => (value, self.paths_for_expr(left)),
            _ => return None,
        };
        let predicates = paths
            .iter()
            .cloned()
            .map(|path| {
                if negated {
                    Predicate::from(Guard::NotEq {
                        path,
                        value: value.clone(),
                    })
                } else {
                    Predicate::from(Guard::Eq {
                        path,
                        value: value.clone(),
                    })
                }
            })
            .collect::<Vec<_>>();
        (!predicates.is_empty()).then(|| Predicate::all(predicates))
    }

    fn comparison_has_unrepresentable_values(&self, args: &[TemplateExpr]) -> bool {
        if !args
            .iter()
            .any(|arg| self.expr_needs_context_value_resolution(arg))
        {
            return false;
        }
        let [left, right] = args else {
            return true;
        };
        !matches!(
            (guard_value_literal(left), guard_value_literal(right)),
            (Some(_), None) | (None, Some(_))
        )
    }
}

fn guard_value_literal(expr: &TemplateExpr) -> Option<GuardValue> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(GuardValue::string(value))
        }
        TemplateExpr::Literal(Literal::Bool(value)) => Some(GuardValue::Bool(*value)),
        TemplateExpr::Literal(Literal::Int(value)) => Some(GuardValue::Int(*value)),
        TemplateExpr::Literal(Literal::Float(value)) => GuardValue::float(*value),
        TemplateExpr::Literal(Literal::Nil) => Some(GuardValue::Null),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/value_path_context/condition_predicate.rs"]
mod tests;
