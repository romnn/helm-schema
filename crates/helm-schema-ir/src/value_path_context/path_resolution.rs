use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::Effects;
use crate::eval_effect::{SelectionPolarity, SelectionReachability, SelectionTruthSource};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{direct_values_path, eval_expr, eval_exprs_effects};
use crate::fragment_expr_eval::{document_result_from_expr, fragment_context_value};
use crate::helper_meta::HelperOutputMeta;
use crate::observed_facts::{HintGrade, ObservedFacts};

use super::{RangeSubject, RangeSubjectIdentity, ValuePathContext};

impl ValuePathContext<'_> {
    pub(crate) fn expression_output_effects(&self, exprs: &[TemplateExpr]) -> Effects {
        let effects = self.expression_effects(exprs);
        let mut values = effects
            .output_value_paths()
            .into_iter()
            .map(|path| helm_schema_core::ValuesPath::parse(&path))
            .collect::<BTreeSet<_>>();
        let defaults = effects.default_paths_with_local();
        let mut observed_facts = ObservedFacts::default();
        if let Some(type_hints) = effects.observed_facts.type_hints.get(&HintGrade::DECLARED) {
            observed_facts
                .type_hints
                .insert(HintGrade::DECLARED, type_hints.clone());
        }
        observed_facts.shape_erased_paths = effects.observed_facts.shape_erased_paths.clone();
        values.extend(defaults.iter().cloned());
        Effects {
            output_paths: values,
            bound_output_paths: effects.bound_output_paths,
            defaults,
            observed_facts,
            encoded_paths: effects.encoded_paths,
            local_output_meta: effects.local_output_meta,
            ..Effects::default()
        }
    }

    pub(crate) fn bound_output_paths_expr(&self, expr: &TemplateExpr) -> Vec<String> {
        self.expression_effects(std::slice::from_ref(expr))
            .bound_output_paths
            .into_iter()
            .map(|path| path.encode())
            .collect()
    }

    fn expression_effects(&self, exprs: &[TemplateExpr]) -> Effects {
        eval_exprs_effects(exprs, self.expression_eval_env())
    }

    pub(super) fn expression_eval_env(&self) -> &EvalEnv {
        &self.eval_env
    }

    pub(crate) fn resolved_values_paths_from_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        eval_expr(expr, self.expression_eval_env())
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
            &self.eval_env.root_fields,
            &self.eval_env.locals,
            &self.eval_env.local_output_meta,
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
        let env = self.expression_eval_env();
        let mut seen = std::collections::HashSet::new();
        let evaluated = document_result_from_expr(
            expr,
            env,
            Some(&self.eval_env.root_fields),
            self.current_dot_fragment.as_ref(),
            self.fragment_context,
            &mut seen,
        );
        let evaluated_truth_reachability = evaluated.output_reachability(SelectionPolarity::Truthy);
        let mut influence_paths = evaluated
            .effects
            .output_value_paths()
            .into_iter()
            .map(|path| helm_schema_core::ValuesPath::parse(&path))
            .collect::<BTreeSet<_>>();
        let value = self
            .with_body_fragment_value_expr(expr)
            .or(evaluated.value)
            .and_then(AbstractValue::without_widened);
        if let Some(value) = &value {
            influence_paths.extend(value.paths());
        }
        let truth_source = evaluated_truth_reachability.truth_source();
        let truth_reachability = match (
            &evaluated.truth,
            value.as_ref().and_then(AbstractValue::static_truthiness),
        ) {
            (crate::scalar_value::TruthCondition::Unknown, Some(true)) => {
                SelectionReachability::always(SelectionTruthSource::RawInput)
            }
            (crate::scalar_value::TruthCondition::Unknown, Some(false)) => {
                SelectionReachability::never(SelectionTruthSource::RawInput)
            }
            _ => SelectionReachability::from((
                &evaluated.truth,
                SelectionPolarity::Truthy,
                truth_source,
            )),
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
            truth_reachability,
            input_identity,
            member_identity,
            member_value,
        }
    }
}

fn range_input_identity(value: &AbstractValue, effects: &Effects) -> Option<RangeSubjectIdentity> {
    let (path, json_decoded) = match value {
        AbstractValue::ValuesPath(path) => (path.clone(), false),
        AbstractValue::JsonDecodedPath(path) => (path.clone(), true),
        AbstractValue::OutputPath(path, meta) => {
            if !meta.json_decoded && !output_meta_preserves_range_shape(meta) {
                return None;
            }
            (path.clone(), meta.json_decoded)
        }
        _ => return None,
    };
    if !json_decoded && !path_preserves_range_shape(&path.encode(), effects) {
        return None;
    }
    Some(RangeSubjectIdentity { path, json_decoded })
}

fn range_member_value(value: &AbstractValue, effects: &Effects) -> Option<AbstractValue> {
    match value {
        AbstractValue::ValuesPath(path) if path_preserves_range_shape(&path.encode(), effects) => {
            let mut path = path.clone();
            path.push_each_member();
            Some(AbstractValue::ValuesPath(path))
        }
        AbstractValue::JsonDecodedPath(path) => {
            let mut path = path.clone();
            path.push_each_member();
            Some(AbstractValue::JsonDecodedPath(path))
        }
        AbstractValue::OutputPath(path, meta)
            if meta.json_decoded
                || meta.nil_scrubbed
                || meta.merge_layers.is_some()
                || output_meta_preserves_range_shape(meta) =>
        {
            let mut meta = meta.clone();
            // Defaulting the collection supplies an empty iterable, not
            // defaults for fields of members that are actually present.
            meta.defaulted = false;
            let mut path = path.clone();
            path.push_each_member();
            Some(AbstractValue::OutputPath(path, meta))
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
        AbstractValue::ValuesPath(path) => {
            let mut path = path.clone();
            path.push_each_member();
            Some(AbstractValue::ValuesPath(path))
        }
        AbstractValue::JsonDecodedPath(path) => {
            let mut path = path.clone();
            path.push_each_member();
            Some(AbstractValue::JsonDecodedPath(path))
        }
        AbstractValue::OutputPath(path, meta) => {
            let mut meta = meta.clone();
            meta.defaulted = false;
            let mut path = path.clone();
            path.push_each_member();
            Some(AbstractValue::OutputPath(path, meta))
        }
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
        identities: &mut BTreeSet<(helm_schema_core::ValuesPath, bool)>,
        has_other_path: &mut bool,
    ) {
        match value {
            AbstractValue::ValuesPath(path) => {
                if let Some(parent) = path.item_parent() {
                    identities.insert((parent, false));
                } else {
                    *has_other_path = true;
                }
            }
            AbstractValue::JsonDecodedPath(path) => {
                if let Some(parent) = path.item_parent() {
                    identities.insert((parent, true));
                } else {
                    *has_other_path = true;
                }
            }
            AbstractValue::OutputPath(path, meta) => {
                if let Some(parent) = path.item_parent() {
                    identities.insert((parent, meta.json_decoded));
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
    !effects
        .observed_facts
        .shape_erased_paths
        .contains(&helm_schema_core::ValuesPath::parse(path))
        && !effects
            .derived_text_paths
            .contains(&helm_schema_core::ValuesPath::parse(path))
        && effects
            .local_output_meta
            .get(&helm_schema_core::ValuesPath::parse(path))
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
        && !meta.json_serialized
        && !meta.nil_scrubbed
        && meta.merge_layers.is_none()
        && meta.lexical_escapes.is_empty()
        && meta.empty_fold_spellings.is_none()
        && meta.empty_rescue.is_none()
        && meta.default_fallback.is_none()
}
