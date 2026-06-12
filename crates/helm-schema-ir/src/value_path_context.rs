use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::binding::{FragmentBinding, HelperBinding};
use crate::expression_analysis::{resolved_default_fallback_paths_for_text, type_is_schema_type};
use crate::fragment_expr_eval::{FragmentEvalContext, fragment_binding_from_outer_expr};
use crate::helper_analysis::HelperOutputMeta;
use crate::helper_binding_eval::{binding_from_expr, resolve_bound_path_expr};
use crate::predicate::{Predicate, PredicateAtom};
use crate::template_expr_analysis::{
    expr_contains_helper_call, walk_expr_excluding_helper_call_args,
};
use crate::template_expr_cache::parse_expr_text;
use crate::walker::{parse_condition, values_path_from_expr};

pub(crate) struct ValuePathContext<'a> {
    pub(crate) root_bindings: &'a HashMap<String, HelperBinding>,
    pub(crate) template_bindings: &'a HashMap<String, FragmentBinding>,
    pub(crate) template_default_paths: &'a HashMap<String, BTreeSet<String>>,
    pub(crate) template_output_meta: &'a HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) fragment_context: FragmentEvalContext<'a>,
    pub(crate) current_dot_fragment: Option<FragmentBinding>,
    pub(crate) current_dot_binding: Option<HelperBinding>,
}

/// Resolves a `with` header's value to the helper binding callers should use as
/// `current_dot` while walking the body.
pub(crate) fn computed_with_body_dot(
    header: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
) -> Option<HelperBinding> {
    let exprs = parse_expr_text(header);
    let [expr] = exprs.as_slice() else {
        return None;
    };

    if is_bare_values_root_expr(expr) {
        return Some(HelperBinding::ValuesPath(String::new()));
    }

    binding_from_expr(expr, Some(bindings), current_dot)
}

impl ValuePathContext<'_> {
    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    pub(crate) fn resolved_values_paths(&self, text: &str) -> Vec<String> {
        let exprs = parse_expr_text(text);
        let mut paths = direct_values_paths_from_exprs(&exprs);

        if !self.root_bindings.is_empty() {
            for expr in &exprs {
                walk_expr_excluding_helper_call_args(expr, &mut |node| {
                    if let Some(path) = resolve_bound_path_expr(node, self.root_bindings) {
                        paths.insert(path);
                    }
                });
            }
        }

        if !self.template_bindings.is_empty() {
            for expr in &exprs {
                walk_expr_excluding_helper_call_args(expr, &mut |node| {
                    paths.extend(self.local_alias_paths_for_expr(node));
                });
            }
        }

        paths.extend(self.resolved_values_paths_in_expr_tree(text));

        paths.into_iter().collect()
    }

    pub(crate) fn resolved_default_fallback_paths(&self, text: &str) -> BTreeSet<String> {
        let mut paths = resolved_default_fallback_paths_for_text(
            text,
            Some(self.root_bindings),
            self.current_dot_binding.as_ref(),
        );
        for expr in parse_expr_text(text) {
            paths.extend(self.resolved_default_fallback_paths_for_expr(&expr));
        }
        if !self.template_default_paths.is_empty() {
            for expr in parse_expr_text(text) {
                expr.walk(|node| {
                    paths.extend(self.local_alias_default_paths_for_expr(node));
                });
            }
        }
        paths
    }

    fn resolved_default_fallback_paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        expr.walk(|node| match node {
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                out.extend(self.resolve_expr_to_values_paths(&args[1]));
            }
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let TemplateExpr::Call { function, .. } = &window[1] else {
                        continue;
                    };
                    if function != "default" {
                        continue;
                    }
                    out.extend(self.resolve_expr_to_values_paths(&window[0]));
                }
            }
            _ => {}
        });
        out
    }

    pub(crate) fn resolve_expr_to_values_paths(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return [path].into_iter().collect();
        }

        let mut locals = self.template_bindings.clone();
        for (key, value) in self.root_bindings {
            locals.insert(key.clone(), value.to_fragment_binding());
        }

        let outer_binding = fragment_binding_from_outer_expr(
            expr,
            Some(&locals),
            Some(self.root_bindings),
            self.current_dot_binding.as_ref(),
        );
        let fragment_binding =
            self.fragment_binding_from_expr(expr, self.current_dot_fragment.as_ref());

        outer_binding
            .into_iter()
            .chain(fragment_binding)
            .flat_map(|binding| FragmentBinding::paths(&binding))
            .filter(|path| !path.trim().is_empty())
            .collect()
    }

    fn fragment_binding_from_expr(
        &self,
        expr: &TemplateExpr,
        current_dot: Option<&FragmentBinding>,
    ) -> Option<FragmentBinding> {
        let mut seen = HashSet::new();
        self.fragment_context.fragment_binding_from_expr(
            expr,
            self.template_bindings,
            current_dot,
            &mut seen,
        )
    }

    pub(crate) fn local_alias_output_meta_for_text(
        &self,
        text: &str,
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        for expr in parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                for (path, meta) in self.local_alias_output_meta_for_expr(node) {
                    out.entry(path).or_default().merge(meta);
                }
            });
        }
        out
    }

    pub(crate) fn condition_predicate(&self, text: &str) -> Predicate {
        let mut predicates: Vec<Predicate> = parse_condition(text)
            .into_iter()
            .map(Predicate::from)
            .collect();
        let alias_predicates = self.condition_predicates_from_aliases(text);
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
        if self.condition_has_unrepresentable_values_comparison(text) {
            return Predicate::True;
        }
        Predicate::all(
            self.resolved_values_paths_in_expr_tree(text)
                .into_iter()
                .map(Predicate::truthy_path)
                .collect(),
        )
    }

    pub(crate) fn with_condition_predicate(&self, text: &str) -> Predicate {
        Predicate::all(with_predicates_from_condition_predicate(
            self.condition_predicate(text),
        ))
    }

    pub(crate) fn with_body_fragment_binding(&self, header: &str) -> Option<FragmentBinding> {
        let mut locals = self.template_bindings.clone();
        for (key, value) in self.root_bindings {
            locals.insert(key.clone(), value.to_fragment_binding());
        }

        let exprs = parse_expr_text(header);
        let [expr] = exprs.as_slice() else {
            return None;
        };
        fragment_binding_from_outer_expr(
            expr,
            Some(&locals),
            Some(self.root_bindings),
            self.current_dot_binding.as_ref(),
        )
        .or_else(|| self.fragment_binding_from_expr(expr, self.current_dot_fragment.as_ref()))
    }

    pub(crate) fn single_resolved_values_path(&self, text: &str) -> Option<String> {
        let mut paths = self.resolved_values_paths(text);
        if paths.len() == 1 { paths.pop() } else { None }
    }

    pub(crate) fn single_direct_iterable_range_path(&self, text: &str) -> Option<String> {
        let exprs = parse_expr_text(text);
        if exprs.len() != 1 || !is_direct_path_expr(&exprs[0], self.root_bindings) {
            return None;
        }
        self.single_resolved_values_path(text)
    }

    fn local_alias_paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_bindings
                .get(var)
                .map(FragmentBinding::paths)
                .unwrap_or_default(),
            TemplateExpr::Selector { operand, path } => match operand.as_ref() {
                TemplateExpr::Variable(var) if !var.is_empty() => self
                    .template_bindings
                    .get(var)
                    .and_then(|binding| binding.apply_to_binding(path))
                    .map(|binding| FragmentBinding::paths(&binding))
                    .unwrap_or_default(),
                _ => BTreeSet::new(),
            },
            _ => BTreeSet::new(),
        }
    }

    fn local_alias_default_paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_default_paths
                .get(var)
                .cloned()
                .unwrap_or_default(),
            _ => BTreeSet::new(),
        }
    }

    fn local_alias_output_meta_for_expr(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeMap<String, HelperOutputMeta> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_output_meta
                .get(var)
                .cloned()
                .unwrap_or_default(),
            TemplateExpr::Selector { operand, path } => {
                let TemplateExpr::Variable(var) = operand.as_ref() else {
                    return BTreeMap::new();
                };
                if var.is_empty() {
                    return BTreeMap::new();
                }
                let Some(binding) = self.template_bindings.get(var) else {
                    return BTreeMap::new();
                };
                let Some(bound) = binding.apply_to_binding(path) else {
                    return BTreeMap::new();
                };
                let selected_paths = FragmentBinding::paths(&bound);
                self.template_output_meta
                    .get(var)
                    .into_iter()
                    .flat_map(|meta_by_path| meta_by_path.iter())
                    .filter(|(path, _meta)| selected_paths.contains(*path))
                    .map(|(path, meta)| (path.clone(), meta.clone()))
                    .collect()
            }
            _ => BTreeMap::new(),
        }
    }

    fn expr_needs_context_value_resolution(&self, expr: &TemplateExpr) -> bool {
        !self.local_alias_paths_for_expr(expr).is_empty()
            || (values_path_from_expr(expr).is_none()
                && !self.resolve_expr_to_values_paths(expr).is_empty())
    }

    fn condition_predicates_from_aliases(&self, text: &str) -> Vec<Predicate> {
        let mut out = Vec::new();
        for expr in parse_expr_text(text) {
            let TemplateExpr::Call { function, args } = expr.deparen() else {
                continue;
            };
            match function.as_str() {
                "not" => {
                    let [arg] = args.as_slice() else {
                        continue;
                    };
                    if !self.expr_needs_context_value_resolution(arg) {
                        continue;
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
                        continue;
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
                        continue;
                    };
                    if !self.expr_needs_context_value_resolution(left)
                        && !self.expr_needs_context_value_resolution(right)
                    {
                        continue;
                    }
                    let (value, paths) =
                        match (owned_string_literal(left), owned_string_literal(right)) {
                            (Some(value), None) => (value, self.paths_for_expr(right)),
                            (None, Some(value)) => (value, self.paths_for_expr(left)),
                            _ => continue,
                        };
                    out.extend(paths.into_iter().map(|path| {
                        Predicate::Atom(PredicateAtom::Eq {
                            path,
                            value: value.clone(),
                        })
                    }));
                }
                "typeIs" => {
                    let Some(schema_type) = type_is_schema_type(args.first()) else {
                        continue;
                    };
                    if !args
                        .iter()
                        .skip(1)
                        .any(|arg| self.expr_needs_context_value_resolution(arg))
                    {
                        continue;
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
            }
        }
        out
    }

    fn paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        let mut paths = self.resolve_expr_to_values_paths(expr);
        paths.extend(self.local_alias_paths_for_expr(expr));
        paths
            .into_iter()
            .filter(|path| !path.trim().is_empty())
            .collect()
    }

    fn condition_has_unrepresentable_values_comparison(&self, text: &str) -> bool {
        parse_expr_text(text).into_iter().any(|expr| {
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
                            borrowed_string_literal(left),
                            borrowed_string_literal(right)
                        ),
                        (Some(_), None) | (None, Some(_))
                    )
                }
                "ne" | "typeIs" => args
                    .iter()
                    .any(|arg| self.expr_needs_context_value_resolution(arg)),
                _ => false,
            }
        })
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn resolved_values_paths_in_expr_tree(&self, text: &str) -> BTreeSet<String> {
        let mut locals = self.template_bindings.clone();
        for (key, value) in self.root_bindings {
            locals.insert(key.clone(), value.to_fragment_binding());
        }

        let mut paths = BTreeSet::new();
        for expr in parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                if expr_contains_helper_call(node) {
                    return;
                }
                let outer_binding = fragment_binding_from_outer_expr(
                    node,
                    Some(&locals),
                    Some(self.root_bindings),
                    self.current_dot_binding.as_ref(),
                );
                let fragment_binding =
                    self.fragment_binding_from_expr(node, self.current_dot_fragment.as_ref());
                paths.extend(
                    outer_binding
                        .into_iter()
                        .chain(fragment_binding)
                        .flat_map(|binding| FragmentBinding::paths(&binding))
                        .filter(|path| !path.trim().is_empty()),
                );
            });
        }
        paths
    }
}

fn is_bare_values_root_expr(expr: &TemplateExpr) -> bool {
    matches!(expr, TemplateExpr::Field(path) if matches!(path.as_slice(), [head] if head == "Values"))
        || matches!(
            expr,
            TemplateExpr::Selector { operand, path }
                if matches!(operand.as_ref(), TemplateExpr::Variable(var) if var.is_empty())
                    && matches!(path.as_slice(), [head] if head == "Values"),
        )
}

fn direct_values_paths_from_exprs(exprs: &[TemplateExpr]) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for expr in exprs {
        walk_expr_excluding_helper_call_args(expr, &mut |node| {
            if let Some(path) = values_path_from_expr(node) {
                paths.insert(path);
            }
        });
    }
    paths
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
        Predicate::Atom(
            PredicateAtom::Range { .. }
            | PredicateAtom::With { .. }
            | PredicateAtom::Default { .. }
            | PredicateAtom::TypeIs { .. },
        ) => vec![predicate],
    }
}

fn is_direct_path_expr(expr: &TemplateExpr, bindings: &HashMap<String, HelperBinding>) -> bool {
    match expr {
        TemplateExpr::Parenthesized(inner) => is_direct_path_expr(inner, bindings),
        TemplateExpr::Field(_) => true,
        TemplateExpr::Selector { .. } => resolve_bound_path_expr(expr, bindings).is_some(),
        _ => false,
    }
}

fn owned_string_literal(arg: &TemplateExpr) -> Option<String> {
    match arg.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(value.clone())
        }
        _ => None,
    }
}

fn borrowed_string_literal(arg: &TemplateExpr) -> Option<&str> {
    match arg.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => Some(value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Guard;

    #[test]
    fn with_predicates_preserve_header_projection_semantics() {
        let predicate = Predicate::all(vec![
            Predicate::truthy_path("service.enabled"),
            Predicate::Atom(PredicateAtom::Eq {
                path: "service.type".to_string(),
                value: "ClusterIP".to_string(),
            }),
            Predicate::Or(vec![
                Predicate::truthy_path("service.annotations"),
                Predicate::truthy_path("global.annotations"),
            ]),
            Predicate::truthy_path("service.disabled").negated(),
        ]);

        let with_predicate = Predicate::all(with_predicates_from_condition_predicate(predicate));

        assert_eq!(
            with_predicate.compatibility_guards(),
            vec![
                Guard::With {
                    path: "service.enabled".to_string(),
                },
                Guard::With {
                    path: "service.type".to_string(),
                },
                Guard::Eq {
                    path: "service.type".to_string(),
                    value: "ClusterIP".to_string(),
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
        assert_eq!(
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
