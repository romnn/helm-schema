//! Assignment actions (`$x := …`, `$x = …`) and structural `set`
//! mutations: local bindings, truthiness reductions, member-host
//! conversions, and helper-scope set-call application.

//! Output-hole evaluation: expression holes evaluate through the existing
//! `AbstractValue` lattice (with bound-helper resolution) and lower into
//! fragment nodes; partial scalars combine per-segment arms with a bounded
//! cartesian product; inline `{{ if }}…{{ end }}` regions inside scalars
//! re-parse structurally and become guarded scalar arms.

use helm_schema_ast::{TemplateExpr, parse_expr_text};
use helm_schema_syntax::Span;

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::parse_get_binding_from_exprs;
use crate::eval_effect::MemberHostConversion;
use crate::fragment_assignment::parse_helper_assignment_from_exprs;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_meta::merge_rendered_row_meta;
use crate::{Guard, ValueKind};
use helm_schema_core::Predicate;

use super::eval::Interpreter;
use super::hole_effects::{RenderedDemotion, predicate_applies_to_flowing_path};

/// The values paths whose type an expression describes, preserving the
/// selection branches of a local that can hold one of several paths.
pub(super) fn type_descriptor_sources(
    expr: &TemplateExpr,
    interpreter: &Interpreter<'_>,
    output_meta: &std::collections::BTreeMap<String, crate::helper_meta::HelperOutputMeta>,
) -> Option<std::collections::BTreeMap<String, crate::helper_meta::HelperOutputMeta>> {
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };
    let subject = helm_schema_ast::type_descriptor_call_subject(function, args)?.deparen();
    if !matches!(
        subject,
        TemplateExpr::Field(_) | TemplateExpr::Selector { .. } | TemplateExpr::Variable(_)
    ) {
        return None;
    }
    let paths = interpreter.value_path_context().paths_for_expr(subject);
    if paths.is_empty() {
        return None;
    }
    Some(
        paths
            .into_iter()
            .map(|path| {
                let meta = output_meta.get(&path).cloned().unwrap_or_default();
                (path, meta)
            })
            .collect(),
    )
}

/// Whether the expression's produced VALUE is derived text (a
/// stringification or string transform): the binding's runtime truthiness is
/// then the text's emptiness, which a surviving input value cannot witness.
pub(super) fn rhs_produces_derived_text(expr: &TemplateExpr) -> bool {
    let stage = match expr.deparen() {
        TemplateExpr::Pipeline(stages) => match stages.last() {
            Some(stage) => stage.deparen(),
            None => return false,
        },
        stage => stage,
    };
    let TemplateExpr::Call { function, .. } = stage else {
        return false;
    };
    helm_schema_ast::is_total_stringification_function(function)
        || helm_schema_ast::is_string_transform_function(function)
        || matches!(
            function.as_str(),
            "join" | "printf" | "print" | "println" | "cat"
        )
}

pub(super) fn self_preserving_nonempty_accumulation(expr: &TemplateExpr, variable: &str) -> bool {
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return false;
    };
    if args.len() < 2 {
        return false;
    }
    if function == "concat" {
        let keeps_self = args.iter().any(|arg| {
            matches!(
                arg.deparen(),
                TemplateExpr::Variable(name) if name.trim_start_matches('$') == variable
            )
        });
        let adds_nonempty_literal_list = args.iter().any(|arg| {
            matches!(
                arg.deparen(),
                TemplateExpr::Call { function, args }
                    if matches!(function.as_str(), "list" | "tuple") && !args.is_empty()
            )
        });
        return keeps_self && adds_nonempty_literal_list;
    }
    let same_variable = matches!(
        args[0].deparen(),
        TemplateExpr::Variable(name) if name.trim_start_matches('$') == variable
    );
    same_variable
        && (matches!(
            function.as_str(),
            "append" | "mustAppend" | "prepend" | "mustPrepend"
        ) || function == "print"
            && args[1..].iter().any(|arg| {
                matches!(
                    arg.deparen(),
                    TemplateExpr::Literal(
                        helm_schema_ast::Literal::String(text)
                            | helm_schema_ast::Literal::RawString(text)
                    ) if !text.is_empty()
                )
            }))
}

pub(super) fn or_predicates(left: Predicate, right: Predicate) -> Predicate {
    match (left, right) {
        (Predicate::True, _) | (_, Predicate::True) => Predicate::True,
        (Predicate::False, predicate) | (predicate, Predicate::False) => predicate,
        (left, right) if left == right => left,
        (Predicate::Or(mut left), Predicate::Or(right)) => {
            left.extend(right);
            left.sort();
            left.dedup();
            Predicate::Or(left)
        }
        (Predicate::Or(mut alternatives), predicate)
        | (predicate, Predicate::Or(mut alternatives)) => {
            alternatives.push(predicate);
            alternatives.sort();
            alternatives.dedup();
            Predicate::Or(alternatives)
        }
        (left, right) => Predicate::Or(vec![left, right]),
    }
}

impl Interpreter<'_> {
    /// Assignment actions: bind the local (fragment semantics), refresh its
    /// default/meta facts, and record the right-hand side's reads — the
    /// current pipeline walks assignment bodies in a no-render scope, so all
    /// of its claims are pathless.
    pub(super) fn eval_assignment_span(&mut self, span: Span) {
        let exprs = parse_expr_text(self.text(span));
        if exprs.is_empty() {
            return;
        }
        let previous_site = self.enter_hole_site(span);
        self.eval_assignment_exprs(&exprs);
        self.restore_site(previous_site);
    }

    /// Structural `set` mutations on local dict bindings (`set $ctx "k" v`,
    /// bare or assigned to `$_`) mutate the target local instead of binding
    /// output. Helper bodies rely on this for config-normalization chains;
    /// only the chart-default effects surface (the summary lane never
    /// claimed set-call operand reads).
    pub(super) fn record_dot_member_host_conversions(&mut self, exprs: &[TemplateExpr]) {
        if self.under_approximate_condition() {
            return;
        }
        let Some(target_path) = self
            .current_value_dot()
            .and_then(|value| value.unique_path())
        else {
            return;
        };
        if target_path.is_empty() {
            return;
        }

        let mut assignments = Vec::new();
        for expr in exprs {
            expr.walk(|node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if function != "set" || args.len() != 3 {
                    return;
                }
                let target_is_dot = match args[0].deparen() {
                    TemplateExpr::Field(path) => path.is_empty(),
                    TemplateExpr::Variable(variable) => variable.is_empty(),
                    _ => false,
                };
                if target_is_dot {
                    assignments.push((args[1].clone(), args[2].clone()));
                }
            });
        }

        for (key_expr, value_expr) in assignments {
            let (keys, assigns_object) = {
                let context = self.value_path_context();
                let keys = context
                    .with_body_fragment_value_expr(&key_expr)
                    .map(|value| value.strings())
                    .unwrap_or_default();
                let assigns_object = matches!(
                    context.with_body_fragment_value_expr(&value_expr),
                    Some(AbstractValue::Dict(_))
                );
                (keys, assigns_object)
            };
            if !assigns_object {
                continue;
            }
            for key in keys {
                let path = helm_schema_core::append_value_path(&target_path, &key);
                for predicate in &self.active_predicates {
                    let Predicate::Guard(Guard::TypeIs {
                        path: tested_path,
                        schema_type,
                    }) = predicate
                    else {
                        continue;
                    };
                    if tested_path != &path || schema_type == "object" {
                        continue;
                    }
                    let mut outer_predicates = self
                        .active_predicates
                        .iter()
                        .filter(|outer| *outer != predicate)
                        .cloned()
                        .collect::<Vec<_>>();
                    outer_predicates.sort();
                    outer_predicates.dedup();
                    self.member_host_conversions.insert(MemberHostConversion {
                        path: path.clone(),
                        input_kind: schema_type.clone(),
                        outer_predicates,
                    });
                }
            }
        }
    }

    pub(super) fn absorb_member_host_conversions(
        &mut self,
        conversions: &std::collections::BTreeSet<MemberHostConversion>,
    ) {
        for conversion in conversions {
            let mut outer_predicates = self.active_predicates.clone();
            outer_predicates.extend(conversion.outer_predicates.iter().cloned());
            if outer_predicates
                .iter()
                .any(Predicate::contains_approximation)
            {
                continue;
            }
            outer_predicates.sort();
            outer_predicates.dedup();
            self.member_host_conversions.insert(MemberHostConversion {
                path: conversion.path.clone(),
                input_kind: conversion.input_kind.clone(),
                outer_predicates,
            });
        }
    }

    pub(super) fn apply_helper_scope_set_mutations(&mut self, exprs: &[TemplateExpr]) -> bool {
        self.record_dot_member_host_conversions(exprs);
        if !self.helper_scope {
            return false;
        }
        let current_dot = self.current_dot_fragment();
        let mut seen = self.helper_seen.clone();
        if !crate::fragment_assignment::apply_local_set_mutations_from_exprs(
            exprs,
            &mut self.locals.fragment_values,
            current_dot.as_ref(),
            FragmentEvalContext::new(self.db),
            &mut seen,
        ) {
            return false;
        }
        let effects = crate::expr_eval::eval_helper_exprs_direct_effects(
            exprs,
            &self.root_bindings,
            self.current_value_dot().as_ref(),
        );
        self.chart_defaults_observed
            .extend(effects.chart_default_paths.iter().cloned());
        let mut chart_defaults = effects.chart_default_paths;
        self.locals.append_chart_value_defaults(&mut chart_defaults);
        true
    }

    pub(super) fn eval_assignment_exprs(&mut self, exprs: &[TemplateExpr]) {
        if self.apply_helper_scope_set_mutations(exprs) {
            return;
        }
        if let Some(assignment) = parse_helper_assignment_from_exprs(exprs) {
            let rhs = std::slice::from_ref(&assignment.rhs_expr);
            self.record_required_subjects(rhs);
            let inlined_template_value = self.inline_static_template_value(rhs);
            let rhs_truthy_reduction = {
                let context = self.value_path_context();
                context
                    .condition_lowering_is_faithful(&assignment.rhs_expr)
                    .then(|| context.condition_predicate_expr(&assignment.rhs_expr))
            };
            let output_effects = self.value_path_context().expression_output_effects(rhs);
            let hole = self.eval_hole_exprs(rhs);
            // The binding is the hole value without widened members (an
            // unknown call result is influence, not a values-backed
            // fragment).
            let fragment_value = inlined_template_value
                .or_else(|| hole.value.clone().and_then(AbstractValue::without_widened));
            let previous_truthy_reduction = self
                .locals
                .truthy_reductions
                .get(&assignment.variable)
                .cloned();
            let truthy_reduction = match assignment.kind {
                crate::fragment_assignment::AssignmentKind::Declaration => rhs_truthy_reduction
                    .or_else(|| {
                        // A text-producing RHS (`$m := join "\n" $list`) keeps
                        // its INPUT value for attribution, but the binding's
                        // runtime truthiness is the produced text's emptiness,
                        // which that value cannot witness: a nonempty list can
                        // join to "". Abstain instead of reducing.
                        if rhs_produces_derived_text(&assignment.rhs_expr) {
                            return None;
                        }
                        fragment_value
                            .as_ref()
                            .and_then(AbstractValue::static_truthiness)
                            .map(|truthy| {
                                if truthy {
                                    Predicate::True
                                } else {
                                    Predicate::False
                                }
                            })
                    }),
                crate::fragment_assignment::AssignmentKind::Assignment
                    if self_preserving_nonempty_accumulation(
                        &assignment.rhs_expr,
                        &assignment.variable,
                    ) =>
                {
                    previous_truthy_reduction.map(|previous| {
                        let condition = Predicate::all(self.active_predicates.clone());
                        let exact = !condition.contains_approximation()
                            && condition.value_paths().iter().all(|path| {
                                !path.starts_with('$') && !path.split('.').any(|part| part == "*")
                            });
                        if exact {
                            or_predicates(previous, condition)
                        } else {
                            previous
                        }
                    })
                }
                // A plain reassignment's static truthiness is branch-local
                // state; the branch join unions it with the other arms'
                // reductions (`$shouldContinue = false` in a traversal's
                // kill-switch arm).
                crate::fragment_assignment::AssignmentKind::Assignment => rhs_truthy_reduction
                    .or_else(|| {
                        if rhs_produces_derived_text(&assignment.rhs_expr) {
                            return None;
                        }
                        fragment_value
                            .as_ref()
                            .and_then(AbstractValue::static_truthiness)
                            .map(|truthy| {
                                if truthy {
                                    Predicate::True
                                } else {
                                    Predicate::False
                                }
                            })
                    }),
            };
            let previous_fragment_value = self
                .locals
                .fragment_values
                .get(&assignment.variable)
                .cloned();
            // Helper bodies keep the prior binding when the right-hand side
            // resolves to nothing (the summary lane's rule): an unresolvable
            // re-assignment in one branch must not erase the other branches'
            // value at the join.
            if fragment_value.is_some() || !self.helper_scope {
                self.locals.bind_fragment_value(
                    assignment.kind,
                    assignment.variable.clone(),
                    fragment_value.clone(),
                );
            }
            // A guarded self-advance (`$x = index $x $k` reassigning `$x`
            // one member deeper while this step's `hasKey` presence guard
            // is active) marks the local so the branch join keeps the
            // advanced value: consumers stay a finite exact path, and
            // their facts carry the member's presence guard.
            if assignment.kind == crate::fragment_assignment::AssignmentKind::Assignment
                && let (
                    Some(AbstractValue::ValuesPath(parent)),
                    Some(AbstractValue::ValuesPath(child)),
                ) = (&previous_fragment_value, &fragment_value)
                && helm_schema_core::values_path_is_descendant(child, parent)
            {
                let presence = Predicate::from(crate::Guard::Absent {
                    path: child.clone(),
                })
                .negated();
                let guarded = self.active_predicates.iter().any(|active| match active {
                    Predicate::And(items) => items.contains(&presence),
                    other => other == &presence,
                });
                if guarded {
                    self.locals.mark_traversal_advance(&assignment.variable);
                }
            }
            if let Some(predicate) = truthy_reduction {
                self.locals
                    .truthy_reductions
                    .insert(assignment.variable.clone(), predicate);
            } else {
                self.locals.truthy_reductions.remove(&assignment.variable);
            }
            // `$tp := typeOf .Values.x` binds a TYPE DESCRIPTOR of the path:
            // later `eq $tp "string"` comparisons are type tests, never value
            // equalities, so remember the described path. Recorded after the
            // value binding, whose displacement clears every other domain.
            if let Some(sources) =
                type_descriptor_sources(&assignment.rhs_expr, self, &hole.effects.local_output_meta)
            {
                self.locals
                    .typeof_sources
                    .insert(assignment.variable.clone(), sources);
            }
            // `$replicas := int (default 1 .Values.x)` binds an INTEGER
            // COERCION of the path: comparisons on the local strengthen
            // through the raw-integer sound subsets exactly as the inline
            // cast expression would (jenkins' controller.replicas domain).
            let cast_source = self
                .value_path_context()
                .int_cast_operand(&assignment.rhs_expr);
            if let Some(source) = cast_source {
                self.locals
                    .int_cast_sources
                    .insert(assignment.variable.clone(), source);
            }
            let mut output_meta = output_effects.local_output_meta.clone();
            merge_rendered_row_meta(&mut output_meta, &hole.effects.helper_rendered);
            if let Some(binding) = &fragment_value {
                for (path, meta) in binding.output_meta() {
                    output_meta.entry(path).or_default().merge(&meta);
                }
            }
            // A shape-erasing RHS (`$tag := … | toString`) rides the binding:
            // wherever the local renders, the splice exposes no input shape.
            for path in &hole.effects.shape_erased_paths {
                output_meta.entry(path.clone()).or_default().shape_erased = true;
            }
            for path in &hole.effects.yaml_serialized_paths {
                output_meta.entry(path.clone()).or_default().yaml_serialized = true;
            }
            // Likewise a derived-text RHS (`$port := include … .`): a later
            // consuming transform on the local operates on rendered text and
            // claims nothing about the underlying paths.
            for path in &hole.effects.derived_text_paths {
                output_meta.entry(path.clone()).or_default().derived_text = true;
            }
            // An omitting RHS (`$ctx = omit $ctx "runAsUser"`) rides the
            // binding: wherever the local renders the map, the removed
            // keys' sink typing must not bind. Retain guards start empty;
            // the branch join fills them where the omit provably did not
            // run.
            for (path, keys) in &hole.effects.omitted_map_keys {
                let meta = output_meta.entry(path.clone()).or_default();
                for key in keys {
                    meta.omitted_keys.insert(key.clone(), Vec::new());
                }
            }
            // A string-contracting RHS (`$name := .Values.x | trunc 63`)
            // also rides the binding: wherever the local renders, that row
            // requires a string input.
            for path in &hole.effects.string_contract_paths {
                output_meta.entry(path.clone()).or_default().string_contract = true;
            }
            for path in &hole.effects.json_serialized_paths {
                output_meta.entry(path.clone()).or_default().json_serialized = true;
            }
            // Eager helper arguments execute, but their rendered values are dependencies rather
            // than part of the assignment's value. Keep their runtime effects while preventing
            // their output metadata from riding the assigned local.
            let fragment_paths = fragment_value
                .as_ref()
                .map(AbstractValue::fragment_rendered_paths)
                .unwrap_or_default();
            let dependency_only_paths: std::collections::BTreeSet<&String> = hole
                .effects
                .helper_dependency_rendered
                .iter()
                .map(|row| &row.path)
                .filter(|path| !fragment_paths.contains(*path))
                .collect();
            output_meta.retain(|path, _| !dependency_only_paths.contains(path));
            // A `=` re-assignment under branch predicates keeps those
            // predicates on each flowing path's meta: the write-through
            // survives the branch join in the locals, so the conditions must
            // ride the meta to the render site. A truthiness condition about
            // a *different* flowing path describes a sibling's branch and
            // stays off this path's meta.
            if assignment.kind == crate::fragment_assignment::AssignmentKind::Assignment
                && !self.active_predicates.is_empty()
            {
                if let Some(binding) = &fragment_value {
                    for path in binding.fragment_rendered_paths() {
                        output_meta.entry(path).or_default();
                    }
                }
                let flowing: std::collections::BTreeSet<String> =
                    output_meta.keys().cloned().collect();
                for (path, meta) in &mut output_meta {
                    let site: std::collections::BTreeSet<Predicate> = self
                        .active_predicates
                        .iter()
                        .filter(|predicate| {
                            predicate_applies_to_flowing_path(predicate, path, &flowing)
                        })
                        .cloned()
                        .collect();
                    meta.conjoin_branches(&site);
                }
            }
            // Keep the (possibly empty) default and meta entries: the branch
            // join unions per-variable facts only for variables every outcome
            // still tracks, and a pre-branch binding without facts must not
            // erase a branch's recorded ones.
            self.locals
                .default_paths
                .insert(assignment.variable.clone(), output_effects.defaults.clone());
            self.locals
                .output_meta
                .insert(assignment.variable.clone(), output_meta);
            let demotion = if self.helper_scope {
                RenderedDemotion::Dependency
            } else {
                RenderedDemotion::Document
            };
            self.absorb_hole_effects(&hole.effects, demotion);
            // Inside helper bodies, direct expression paths ride the binding
            // and surface where the local renders; the summary lane never
            // claimed them at the assignment itself.
            if !self.helper_scope {
                let kind = if rhs.iter().any(TemplateExpr::renders_yaml_fragment) {
                    ValueKind::Fragment
                } else {
                    ValueKind::Scalar
                };
                self.push_effects_reads(&hole, kind);
            }
        }
        if let Some(get_binding) = parse_get_binding_from_exprs(exprs) {
            self.locals.apply_get_binding(get_binding);
        }
    }
}
