use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::Guard;
use crate::condition_guards::{guard_value_literal, parse_condition_expr};
use crate::expr_function_catalog::type_is_schema_type;
use crate::predicate::Predicate;

use super::ValuePathContext;

impl ValuePathContext<'_> {
    pub(crate) fn condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        let mut predicates: Vec<Predicate> = parse_condition_expr(expr)
            .into_iter()
            .map(Predicate::from)
            .collect();
        let structural_paths = predicates
            .iter()
            .flat_map(Predicate::value_paths)
            .collect::<BTreeSet<_>>();
        let mut alias_predicates = self.condition_predicates_from_expr(expr);
        alias_predicates.retain(|predicate| {
            !truthy_predicate_is_covered_by_structural_paths(predicate, &structural_paths)
        });
        predicates.retain(|predicate| {
            !predicate_is_subsumed_by_alias_or_predicate(predicate, &alias_predicates)
        });
        for predicate in alias_predicates {
            if !predicates.contains(&predicate) {
                predicates.push(predicate);
            }
        }
        if !predicates.is_empty() {
            return Predicate::all(predicates);
        }
        if self.condition_has_unrepresentable_values_comparison_expr(expr) {
            return Predicate::True;
        }
        Predicate::all(
            self.truthy_paths_for_condition_expr(expr)
                .into_iter()
                .map(Predicate::truthy_path)
                .collect(),
        )
    }

    pub(crate) fn with_condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        Predicate::all(
            self.condition_predicate_expr(expr)
                .with_context_predicates(),
        )
    }

    fn condition_predicates_from_expr(&self, expr: &TemplateExpr) -> Vec<Predicate> {
        let mut out = Vec::new();
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return out;
        };
        match function.as_str() {
            "not" => {
                let [arg] = args.as_slice() else {
                    return out;
                };
                if !self.expr_needs_context_value_resolution(arg) {
                    return out;
                }
                let paths = self.paths_for_expr(arg);
                out.extend(
                    paths
                        .into_iter()
                        .map(|path| Predicate::truthy_path(path).negated()),
                );
            }
            "or" => {
                if !args
                    .iter()
                    .any(|arg| self.expr_needs_context_value_resolution(arg))
                {
                    return out;
                }
                let paths: BTreeSet<String> = args
                    .iter()
                    .flat_map(|arg| self.paths_for_expr(arg))
                    .collect();
                if !paths.is_empty() {
                    out.push(Predicate::Or(
                        paths.into_iter().map(Predicate::truthy_path).collect(),
                    ));
                }
            }
            "eq" => {
                out.extend(self.value_comparison_predicates(args, false));
            }
            "ne" => {
                out.extend(self.value_comparison_predicates(args, true));
            }
            "typeIs" => {
                let Some(schema_type) = type_is_schema_type(args.first()) else {
                    return out;
                };
                if !args
                    .iter()
                    .skip(1)
                    .any(|arg| self.expr_needs_context_value_resolution(arg))
                {
                    return out;
                }
                let paths: BTreeSet<String> = args
                    .iter()
                    .skip(1)
                    .flat_map(|arg| self.paths_for_expr(arg))
                    .collect();
                out.extend(paths.into_iter().map(|path| {
                    Predicate::from(Guard::TypeIs {
                        path,
                        schema_type: schema_type.clone(),
                    })
                }));
            }
            _ => {}
        };
        out
    }

    fn condition_has_unrepresentable_values_comparison_expr(&self, expr: &TemplateExpr) -> bool {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return false;
        };
        match function.as_str() {
            "eq" | "ne" => self.comparison_has_unrepresentable_values(args),
            "typeIs" => args
                .iter()
                .any(|arg| self.expr_needs_context_value_resolution(arg)),
            _ => false,
        }
    }

    fn value_comparison_predicates(&self, args: &[TemplateExpr], negated: bool) -> Vec<Predicate> {
        let [left, right] = args else {
            return Vec::new();
        };
        if !self.expr_needs_context_value_resolution(left)
            && !self.expr_needs_context_value_resolution(right)
        {
            return Vec::new();
        }
        let (value, paths) = match (guard_value_literal(left), guard_value_literal(right)) {
            (Some(value), None) => (value, self.paths_for_expr(right)),
            (None, Some(value)) => (value, self.paths_for_expr(left)),
            _ => return Vec::new(),
        };
        paths
            .into_iter()
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
            .collect()
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

fn truthy_predicate_is_covered_by_structural_paths(
    predicate: &Predicate,
    structural_paths: &BTreeSet<String>,
) -> bool {
    let Some(paths) = predicate.truthy_disjunction_paths() else {
        return false;
    };
    paths.iter().all(|path| structural_paths.contains(path))
}

fn predicate_is_subsumed_by_alias_or_predicate(
    predicate: &Predicate,
    alias_predicates: &[Predicate],
) -> bool {
    let Some(paths) = predicate.truthy_disjunction_paths() else {
        return false;
    };

    alias_predicates.iter().any(|alias_predicate| {
        let Some(alias_paths) = alias_predicate.truthy_disjunction_paths() else {
            return false;
        };
        paths
            .iter()
            .all(|path| alias_paths.iter().any(|alias_path| alias_path == path))
    })
}

#[cfg(test)]
#[path = "../tests/value_path_context/condition_predicate.rs"]
mod tests;
