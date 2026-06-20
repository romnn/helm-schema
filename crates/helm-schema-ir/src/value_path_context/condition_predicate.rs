use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::GuardValue;
use crate::condition_guards::parse_condition_expr;
use crate::expression_analysis::type_is_schema_type;
use crate::predicate::{Predicate, PredicateAtom};
use crate::value_path_extraction::values_path_from_expr;

use super::ValuePathContext;

impl ValuePathContext<'_> {
    pub(crate) fn condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        let mut predicates: Vec<Predicate> = parse_condition_expr(expr)
            .into_iter()
            .map(Predicate::from)
            .collect();
        let alias_predicates = self.condition_predicates_from_expr(expr);
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
            self.resolved_values_paths_in_expr_tree(expr)
                .into_iter()
                .map(Predicate::truthy_path)
                .collect(),
        )
    }

    pub(crate) fn with_condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        Predicate::all(with_predicates_from_condition_predicate(
            self.condition_predicate_expr(expr),
        ))
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
                let [left, right] = args.as_slice() else {
                    return out;
                };
                if !self.expr_needs_context_value_resolution(left)
                    && !self.expr_needs_context_value_resolution(right)
                {
                    return out;
                }
                let (value, paths) = match (
                    owned_guard_value_literal(left),
                    owned_guard_value_literal(right),
                ) {
                    (Some(value), None) => (value, self.paths_for_expr(right)),
                    (None, Some(value)) => (value, self.paths_for_expr(left)),
                    _ => return out,
                };
                out.extend(paths.into_iter().map(|path| {
                    Predicate::Atom(PredicateAtom::Eq {
                        path,
                        value: value.clone(),
                    })
                }));
            }
            "ne" => {
                let [left, right] = args.as_slice() else {
                    return out;
                };
                if !self.expr_needs_context_value_resolution(left)
                    && !self.expr_needs_context_value_resolution(right)
                {
                    return out;
                }
                let (value, paths) = match (
                    owned_guard_value_literal(left),
                    owned_guard_value_literal(right),
                ) {
                    (Some(value), None) => (value, self.paths_for_expr(right)),
                    (None, Some(value)) => (value, self.paths_for_expr(left)),
                    _ => return out,
                };
                out.extend(paths.into_iter().map(|path| {
                    Predicate::Atom(PredicateAtom::NotEq {
                        path,
                        value: value.clone(),
                    })
                }));
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
                    Predicate::Atom(PredicateAtom::TypeIs {
                        path,
                        schema_type: schema_type.clone(),
                    })
                }));
            }
            _ => {}
        };
        out
    }

    fn expr_needs_context_value_resolution(&self, expr: &TemplateExpr) -> bool {
        !self.local_alias_paths_for_expr(expr).is_empty()
            || (values_path_from_expr(expr).is_none()
                && !self.resolve_expr_to_values_paths(expr).is_empty())
    }

    fn condition_has_unrepresentable_values_comparison_expr(&self, expr: &TemplateExpr) -> bool {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return false;
        };
        match function.as_str() {
            "eq" => {
                let has_values_path = args
                    .iter()
                    .any(|arg| self.expr_needs_context_value_resolution(arg));
                if !has_values_path {
                    return false;
                }
                let [left, right] = args.as_slice() else {
                    return true;
                };
                !matches!(
                    (
                        borrowed_guard_value_literal(left),
                        borrowed_guard_value_literal(right)
                    ),
                    (Some(_), None) | (None, Some(_))
                )
            }
            "ne" => {
                let has_values_path = args
                    .iter()
                    .any(|arg| self.expr_needs_context_value_resolution(arg));
                if !has_values_path {
                    return false;
                }
                let [left, right] = args.as_slice() else {
                    return true;
                };
                !matches!(
                    (
                        borrowed_guard_value_literal(left),
                        borrowed_guard_value_literal(right)
                    ),
                    (Some(_), None) | (None, Some(_))
                )
            }
            "typeIs" => args
                .iter()
                .any(|arg| self.expr_needs_context_value_resolution(arg)),
            _ => false,
        }
    }
}

fn predicate_is_subsumed_by_alias_or_predicate(
    predicate: &Predicate,
    alias_predicates: &[Predicate],
) -> bool {
    let Some(paths) = truthy_or_predicate_paths(predicate) else {
        return false;
    };

    alias_predicates.iter().any(|alias_predicate| {
        let Some(alias_paths) = truthy_or_predicate_paths(alias_predicate) else {
            return false;
        };
        paths
            .iter()
            .all(|path| alias_paths.iter().any(|alias_path| alias_path == path))
    })
}

fn truthy_or_predicate_paths(predicate: &Predicate) -> Option<Vec<String>> {
    match predicate {
        Predicate::Atom(PredicateAtom::Truthy { path }) => Some(vec![path.clone()]),
        Predicate::Or(predicates) => truthy_predicate_paths(predicates),
        _ => None,
    }
}

fn truthy_predicate_paths(predicates: &[Predicate]) -> Option<Vec<String>> {
    predicates
        .iter()
        .map(|predicate| match predicate {
            Predicate::Atom(PredicateAtom::Truthy { path }) => Some(path.clone()),
            _ => None,
        })
        .collect()
}

fn with_predicates_from_condition_predicate(predicate: Predicate) -> Vec<Predicate> {
    match predicate {
        Predicate::True => Vec::new(),
        Predicate::False => vec![Predicate::False],
        Predicate::And(predicates) => predicates
            .into_iter()
            .flat_map(with_predicates_from_condition_predicate)
            .collect(),
        Predicate::Atom(PredicateAtom::Truthy { path }) => {
            vec![Predicate::Atom(PredicateAtom::With { path })]
        }
        Predicate::Or(predicates) => {
            let Some(paths) = truthy_predicate_paths(&predicates) else {
                return vec![Predicate::Or(predicates)];
            };
            let mut out: Vec<Predicate> = paths
                .iter()
                .map(|path| Predicate::Atom(PredicateAtom::With { path: path.clone() }))
                .collect();
            out.push(Predicate::Or(
                paths.into_iter().map(Predicate::truthy_path).collect(),
            ));
            out
        }
        Predicate::Not(inner) => match inner.as_ref() {
            Predicate::Atom(PredicateAtom::Truthy { path }) => vec![
                Predicate::Atom(PredicateAtom::With { path: path.clone() }),
                Predicate::Not(inner),
            ],
            _ => vec![Predicate::Not(inner)],
        },
        Predicate::Atom(PredicateAtom::Eq { path, value }) => vec![
            Predicate::Atom(PredicateAtom::With { path: path.clone() }),
            Predicate::Atom(PredicateAtom::Eq { path, value }),
        ],
        Predicate::Atom(PredicateAtom::NotEq { path, value }) => vec![
            Predicate::Atom(PredicateAtom::With { path: path.clone() }),
            Predicate::Atom(PredicateAtom::NotEq { path, value }),
        ],
        Predicate::Atom(
            PredicateAtom::Range { .. }
            | PredicateAtom::Absent { .. }
            | PredicateAtom::With { .. }
            | PredicateAtom::Default { .. }
            | PredicateAtom::TypeIs { .. },
        ) => vec![predicate],
    }
}

fn owned_guard_value_literal(arg: &TemplateExpr) -> Option<GuardValue> {
    match arg.deparen() {
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

fn borrowed_guard_value_literal(arg: &TemplateExpr) -> Option<()> {
    match arg.deparen() {
        TemplateExpr::Literal(
            Literal::String(_)
            | Literal::RawString(_)
            | Literal::Bool(_)
            | Literal::Int(_)
            | Literal::Float(_)
            | Literal::Nil,
        ) => Some(()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Guard;
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn with_predicates_preserve_header_projection_semantics() {
        let predicate = Predicate::all(vec![
            Predicate::truthy_path("service.enabled"),
            Predicate::Atom(PredicateAtom::Eq {
                path: "service.type".to_string(),
                value: GuardValue::string("ClusterIP"),
            }),
            Predicate::Or(vec![
                Predicate::truthy_path("service.annotations"),
                Predicate::truthy_path("global.annotations"),
            ]),
            Predicate::truthy_path("service.disabled").negated(),
        ]);

        let with_predicate = Predicate::all(with_predicates_from_condition_predicate(predicate));

        sim_assert_eq!(
            with_predicate.contract_guards(),
            vec![
                Guard::With {
                    path: "service.enabled".to_string(),
                },
                Guard::With {
                    path: "service.type".to_string(),
                },
                Guard::Eq {
                    path: "service.type".to_string(),
                    value: GuardValue::string("ClusterIP"),
                },
                Guard::With {
                    path: "service.annotations".to_string(),
                },
                Guard::With {
                    path: "global.annotations".to_string(),
                },
                Guard::Or {
                    paths: vec![
                        "service.annotations".to_string(),
                        "global.annotations".to_string(),
                    ],
                },
                Guard::With {
                    path: "service.disabled".to_string(),
                },
                Guard::Not {
                    path: "service.disabled".to_string(),
                },
            ]
        );
        sim_assert_eq!(
            Predicate::all(with_predicates_from_condition_predicate(Predicate::False)),
            Predicate::False,
        );
    }

    #[test]
    fn alias_or_predicate_subsumes_direct_truthy_predicates() {
        let alias_predicate = Predicate::Or(vec![
            Predicate::truthy_path("service.annotations"),
            Predicate::truthy_path("global.annotations"),
        ]);

        assert!(predicate_is_subsumed_by_alias_or_predicate(
            &Predicate::truthy_path("service.annotations"),
            std::slice::from_ref(&alias_predicate),
        ));
        assert!(predicate_is_subsumed_by_alias_or_predicate(
            &Predicate::Or(vec![
                Predicate::truthy_path("service.annotations"),
                Predicate::truthy_path("global.annotations"),
            ]),
            &[alias_predicate],
        ));
        assert!(!predicate_is_subsumed_by_alias_or_predicate(
            &Predicate::truthy_path("service.labels"),
            &[],
        ));
    }
}
