use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::BoundValueContext;
use crate::eval_effect::Effects;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{direct_values_path, eval_expr, eval_exprs_effects};
use crate::fragment_expr_eval::fragment_context_value;
use crate::helper_meta::HelperOutputMeta;
use crate::scalar_value::TruthCondition;

use super::{RangeSubject, RangeSubjectIdentity, ValuePathContext};

impl ValuePathContext<'_> {
    pub(crate) fn expression_output_effects(&self, exprs: &[TemplateExpr]) -> Effects {
        let effects = self.expression_effects(exprs);
        let mut values = effects.output_value_paths();
        let defaults = effects.default_paths_with_local();
        let type_hints = effects.type_hints.clone();
        values.extend(defaults.iter().cloned());
        Effects {
            output_paths: values,
            bound_output_paths: effects.bound_output_paths,
            defaults,
            type_hints,
            encoded_paths: effects.encoded_paths,
            shape_erased_paths: effects.shape_erased_paths,
            local_output_meta: effects.local_output_meta,
            ..Effects::default()
        }
    }

    pub(crate) fn bound_output_paths_expr(&self, expr: &TemplateExpr) -> Vec<String> {
        self.expression_effects(std::slice::from_ref(expr))
            .bound_output_paths
            .into_iter()
            .collect()
    }

    fn expression_effects(&self, exprs: &[TemplateExpr]) -> Effects {
        let env = self.expression_eval_env();
        eval_exprs_effects(exprs, &env)
    }

    pub(super) fn expression_eval_env(&self) -> EvalEnv {
        let current_dot = self
            .current_dot_fragment
            .as_ref()
            .map(AbstractValue::to_context_value)
            .or_else(|| self.current_dot_binding.clone());
        let mut env = EvalEnv::from_helper_context(Some(self.root_bindings), current_dot.as_ref())
            .without_helper_call_args();
        // Locals and root bindings are distinct namespaces (see the hole
        // evaluator); roots resolve through `root_fields` only.
        env.locals = self.template_bindings.clone();
        env.pipeline_bound_locals = self.pipeline_bound_bindings.clone();
        env.local_default_paths = self.template_default_paths.clone();
        env.local_output_meta = self.template_output_meta.clone();
        env.local_scalar_dispatches = self.template_scalar_dispatches.clone();
        env.local_truthy_reductions = self.template_truthy_reductions.clone();
        env.root_truthy_predicates = self.root_truthy_predicates.clone();
        env.root_value_dispatches = self.root_value_dispatches.clone();
        env.root_field_semantics_on_current_dot = self.root_field_semantics_on_current_dot;
        env.bound_values = BoundValueContext::new(self.range_domains, self.get_bindings);
        env
    }

    pub(crate) fn resolved_values_paths_from_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        eval_expr(expr, &self.expression_eval_env())
            .effects
            .output_value_paths()
    }

    pub(crate) fn paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        self.resolved_values_paths_from_expr(expr)
    }

    pub(super) fn expr_needs_context_value_resolution(&self, expr: &TemplateExpr) -> bool {
        direct_values_path(expr).is_none() && !self.resolved_values_paths_from_expr(expr).is_empty()
    }

    pub(crate) fn with_body_fragment_value_expr(
        &self,
        expr: &TemplateExpr,
    ) -> Option<AbstractValue> {
        fragment_context_value(
            expr,
            self.root_bindings,
            &self.template_bindings,
            self.fragment_context,
            self.current_dot_fragment.as_ref(),
        )
    }

    pub(crate) fn single_resolved_values_path_expr(&self, expr: &TemplateExpr) -> Option<String> {
        let mut paths: Vec<_> = self
            .resolved_values_paths_from_expr(expr)
            .into_iter()
            .collect();
        if paths.len() == 1 { paths.pop() } else { None }
    }

    pub(crate) fn range_subject_expr(&self, expr: &TemplateExpr) -> RangeSubject {
        let evaluated = eval_expr(expr, &self.expression_eval_env());
        let mut influence_paths = evaluated.effects.output_value_paths();
        let value = self
            .with_body_fragment_value_expr(expr)
            .or(evaluated.value)
            .and_then(AbstractValue::without_widened);
        if let Some(value) = &value {
            influence_paths.extend(value.paths());
        }
        let truth = match (
            &evaluated.truth,
            value.as_ref().and_then(AbstractValue::static_truthiness),
        ) {
            (TruthCondition::Unknown, Some(truthy)) => TruthCondition::exact(if truthy {
                helm_schema_core::Predicate::True
            } else {
                helm_schema_core::Predicate::False
            }),
            (truth, _) => truth.clone(),
        };
        let input_identity = value
            .as_ref()
            .and_then(|value| range_input_identity(value, &evaluated.effects));
        let member_value = value
            .as_ref()
            .and_then(|value| range_member_value(value, &evaluated.effects));
        let member_identity = member_value
            .as_ref()
            .and_then(single_member_collection_identity);

        RangeSubject {
            influence_paths,
            value,
            truth,
            input_identity,
            member_identity,
            member_value,
        }
    }
}

fn range_input_identity(value: &AbstractValue, effects: &Effects) -> Option<RangeSubjectIdentity> {
    let (path, json_decoded) = match value {
        AbstractValue::ValuesPath(path) => (path, false),
        AbstractValue::JsonDecodedPath(path) => (path, true),
        AbstractValue::OutputPath(path, meta) => {
            if !meta.json_decoded && !output_meta_preserves_range_shape(meta) {
                return None;
            }
            (path, meta.json_decoded)
        }
        _ => return None,
    };
    if !json_decoded && !path_preserves_range_shape(path, effects) {
        return None;
    }
    Some(RangeSubjectIdentity {
        path: path.clone(),
        json_decoded,
    })
}

fn range_member_value(value: &AbstractValue, effects: &Effects) -> Option<AbstractValue> {
    match value {
        AbstractValue::ValuesPath(path) if path_preserves_range_shape(path, effects) => Some(
            AbstractValue::ValuesPath(helm_schema_core::append_value_path(path, "*")),
        ),
        AbstractValue::JsonDecodedPath(path) => Some(AbstractValue::JsonDecodedPath(
            helm_schema_core::append_value_path(path, "*"),
        )),
        AbstractValue::OutputPath(path, meta)
            if meta.json_decoded
                || meta.nil_scrubbed
                || meta.merge_layers.is_some()
                || output_meta_preserves_range_shape(meta) =>
        {
            Some(AbstractValue::OutputPath(
                helm_schema_core::append_value_path(path, "*"),
                meta.clone(),
            ))
        }
        AbstractValue::KeysList(path) => Some(AbstractValue::RangeKey(path.clone())),
        AbstractValue::List(items) => AbstractValue::choice(items.clone()),
        AbstractValue::Dict(entries) => AbstractValue::choice(entries.values().cloned().collect()),
        AbstractValue::Overlay { entries, fallback } => {
            let mut members = entries.values().cloned().collect::<Vec<_>>();
            members.extend(range_member_value(fallback, effects));
            AbstractValue::choice(members)
        }
        AbstractValue::Choice(choices) => AbstractValue::choice(
            choices
                .iter()
                .filter_map(|choice| range_member_value(choice, effects))
                .collect(),
        ),
        AbstractValue::FirstTruthy(candidates) => AbstractValue::choice(
            candidates
                .iter()
                .filter_map(|candidate| range_member_value(candidate, effects))
                .collect(),
        ),
        AbstractValue::MergedLayers(layers) => AbstractValue::choice(
            layers
                .iter()
                .filter_map(|layer| range_layer_member_value(layer, effects))
                .collect(),
        ),
        AbstractValue::SplitList { .. } | AbstractValue::SplitSegment { .. } => {
            Some(AbstractValue::Unknown)
        }
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::ValuesPath(_)
        | AbstractValue::RangeKey(_)
        | AbstractValue::OutputPath(_, _)
        | AbstractValue::RootContext
        | AbstractValue::StringSet(_)
        | AbstractValue::DerivedBoolean(_)
        | AbstractValue::Widened(_) => None,
    }
}

fn range_layer_member_value(value: &AbstractValue, effects: &Effects) -> Option<AbstractValue> {
    match value {
        AbstractValue::ValuesPath(path) => Some(AbstractValue::ValuesPath(
            helm_schema_core::append_value_path(path, "*"),
        )),
        AbstractValue::JsonDecodedPath(path) => Some(AbstractValue::JsonDecodedPath(
            helm_schema_core::append_value_path(path, "*"),
        )),
        AbstractValue::OutputPath(path, meta) => Some(AbstractValue::OutputPath(
            helm_schema_core::append_value_path(path, "*"),
            meta.clone(),
        )),
        AbstractValue::Choice(choices) => AbstractValue::choice(
            choices
                .iter()
                .filter_map(|choice| range_layer_member_value(choice, effects))
                .collect(),
        ),
        AbstractValue::FirstTruthy(candidates) => AbstractValue::choice(
            candidates
                .iter()
                .filter_map(|candidate| range_layer_member_value(candidate, effects))
                .collect(),
        ),
        AbstractValue::MergedLayers(layers) => AbstractValue::choice(
            layers
                .iter()
                .filter_map(|layer| range_layer_member_value(layer, effects))
                .collect(),
        ),
        other => range_member_value(other, effects),
    }
}

fn single_member_collection_identity(value: &AbstractValue) -> Option<RangeSubjectIdentity> {
    fn collect(
        value: &AbstractValue,
        identities: &mut BTreeSet<(String, bool)>,
        has_other_path: &mut bool,
    ) {
        match value {
            AbstractValue::ValuesPath(path) => {
                if let Some(parent) = path.strip_suffix(".*") {
                    identities.insert((parent.to_string(), false));
                } else {
                    *has_other_path = true;
                }
            }
            AbstractValue::JsonDecodedPath(path) => {
                if let Some(parent) = path.strip_suffix(".*") {
                    identities.insert((parent.to_string(), true));
                } else {
                    *has_other_path = true;
                }
            }
            AbstractValue::OutputPath(path, meta) => {
                if let Some(parent) = path.strip_suffix(".*") {
                    identities.insert((parent.to_string(), meta.json_decoded));
                } else {
                    *has_other_path = true;
                }
            }
            AbstractValue::Choice(choices) => {
                for choice in choices {
                    collect(choice, identities, has_other_path);
                }
            }
            AbstractValue::FirstTruthy(candidates) => {
                for candidate in candidates {
                    collect(candidate, identities, has_other_path);
                }
            }
            AbstractValue::MergedLayers(layers) => {
                for layer in layers {
                    collect(layer, identities, has_other_path);
                }
            }
            AbstractValue::Overlay { entries, fallback } => {
                for entry in entries.values() {
                    collect(entry, identities, has_other_path);
                }
                collect(fallback, identities, has_other_path);
            }
            AbstractValue::Dict(entries) => {
                for entry in entries.values() {
                    collect(entry, identities, has_other_path);
                }
            }
            AbstractValue::List(items) => {
                for item in items {
                    collect(item, identities, has_other_path);
                }
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::RangeKey(_)
            | AbstractValue::KeysList(_)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::DerivedBoolean(_)
            | AbstractValue::SplitList { .. }
            | AbstractValue::SplitSegment { .. }
            | AbstractValue::Widened(_) => {}
        }
    }

    let mut identities = BTreeSet::new();
    let mut has_other_path = false;
    collect(value, &mut identities, &mut has_other_path);
    if has_other_path {
        return None;
    }
    let mut identities = identities.into_iter();
    let (Some((path, json_decoded)), None) = (identities.next(), identities.next()) else {
        return None;
    };
    Some(RangeSubjectIdentity { path, json_decoded })
}

fn path_preserves_range_shape(path: &str, effects: &Effects) -> bool {
    !effects.shape_erased_paths.contains(path)
        && !effects.derived_text_paths.contains(path)
        && effects
            .local_output_meta
            .get(path)
            .is_none_or(output_meta_preserves_range_shape)
}

fn output_meta_preserves_range_shape(meta: &HelperOutputMeta) -> bool {
    !meta.defaulted
        && !meta.shape_erased
        && !meta.nil_omitted
        && !meta.stringified
        && !meta.yaml_serialized
        && !meta.derived_text
        && !meta.partial_text
        && !meta.string_contract
        && !meta.json_serialized
        && !meta.nil_scrubbed
        && meta.merge_layers.is_none()
        && meta.omitted_keys.is_empty()
        && meta.lexical_escapes.is_empty()
        && meta.empty_fold_spellings.is_none()
        && meta.empty_rescue.is_none()
        && meta.default_fallback.is_none()
}
