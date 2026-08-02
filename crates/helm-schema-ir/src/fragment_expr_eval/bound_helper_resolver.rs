use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::CustomMergeHelper;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};

use super::{FragmentEvalContext, document_result_from_expr};

pub(super) fn eval_expr_result_with_bound_helpers(
    expr: &TemplateExpr,
    env: &EvalEnv,
    params: BoundHelperValueResolverParams<'_, '_, '_>,
) -> EvalResult {
    let mut resolver = BoundHelperValueResolver {
        caller_env: env,
        params,
    };
    eval_expr_with_helper_calls(expr, env, &mut resolver)
}

pub(super) struct BoundHelperValueResolverParams<'a, 'context, 'seen> {
    pub(super) outer: Option<&'a HashMap<String, AbstractValue>>,
    pub(super) current_dot: Option<&'a AbstractValue>,
    pub(super) context: FragmentEvalContext<'context>,
    pub(super) seen: &'seen mut HashSet<String>,
}

struct BoundHelperValueResolver<'env, 'a, 'context, 'seen> {
    caller_env: &'env EvalEnv,
    params: BoundHelperValueResolverParams<'a, 'context, 'seen>,
}

impl HelperCallValueResolver for BoundHelperValueResolver<'_, '_, '_, '_> {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        arg: Option<&TemplateExpr>,
    ) -> Option<EvalResult> {
        if !self.params.context.analysis_db.has_helper(name) {
            return None;
        }
        if let Some(result) = self.custom_merge_call(name, arg) {
            return Some(result);
        }
        if let Some(result) = self.nil_scrub_call(name, arg) {
            return Some(result);
        }
        if self.params.seen.contains(name) {
            return Some(EvalResult::none());
        }
        let call = self.params.context.analysis_db.summarize_bound_helper_call(
            name,
            arg,
            self.params.outer,
            self.params.current_dot,
            self.caller_env,
            self.params.context,
            self.params.seen,
        );
        let summary = &call.summary;
        // The resolver boundary is the one place summary facts enter
        // expression effects; collectors read the Effects fields only.
        // Encoded rows surface as encoded paths so value-lattice lowerings
        // keep the "sink does not constrain the value" semantics the row
        // recorded (the projected value's output paths carry no encoding
        // flag).
        let mut effects = Effects {
            chart_default_paths: summary.chart_defaults.clone(),
            root_set_mutations: summary.root_set_mutations.clone(),
            root_set_predicates: summary.root_set_predicates.clone(),
            root_set_value_dispatches: summary.root_set_value_dispatches.clone(),
            values_default_sources: summary.values_default_sources.clone(),
            type_hints: summary.type_hints.clone(),
            guarded_type_hints: summary.guarded_type_hints.clone(),
            parsed_yaml_input_paths: summary.parsed_yaml_input_paths.clone(),
            yaml_serialized_paths: summary.yaml_serialized_paths.clone(),
            json_serialized_paths: summary
                .rendered
                .iter()
                .filter(|row| row.meta.json_serialized)
                .map(|row| row.path.clone())
                .collect(),
            encoded_paths: summary.encoded_paths(),
            helper_observed_shape_erased_paths: summary.shape_erased_paths.clone(),
            string_contract_paths: summary.string_contract_paths.clone(),
            range_modes: summary.range_modes.clone(),
            // An include renders its body to text, so every path the value
            // carries is derived text at the call site: a consuming stage
            // (`include … | trimAll`) must not claim contracts on the
            // helper's internal paths.
            derived_text_paths: summary
                .value
                .as_ref()
                .map(AbstractValue::paths)
                .unwrap_or_default(),
            helper_reads: summary.reads.clone(),
            helper_rendered: summary.rendered.clone(),
            helper_suppressed_paths: summary.suppress_predicate_paths.clone(),
            helper_fails: summary.fail_conditions.clone(),
            helper_text_fails: summary.text_fails.clone(),
            member_host_conversions: summary.member_host_conversions.clone(),
            ..Effects::default()
        };
        effects.merge(call.argument_effects);
        // Helper arguments execute first, so a body mutation of the same root
        // field is the value visible after the call returns.
        for key in summary.root_set_mutations.keys() {
            effects.root_set_predicates.remove(key);
            effects.root_set_value_dispatches.remove(key);
        }
        effects
            .root_set_mutations
            .extend(summary.root_set_mutations.clone());
        effects
            .root_set_predicates
            .extend(summary.root_set_predicates.clone());
        effects
            .root_set_value_dispatches
            .extend(summary.root_set_value_dispatches.clone());
        let mut result = EvalResult::with_effects(summary.value.clone(), effects);
        result.json_payload_truth = summary.json_payload_truth.clone();
        Some(match &summary.scalar_dispatch {
            Some(dispatch) => result.with_scalar_dispatch(dispatch.clone()),
            None => result,
        })
    }

    fn resolve_implicit_template_call(
        &mut self,
        suffix: &str,
        arg: Option<&TemplateExpr>,
    ) -> Option<EvalResult> {
        let name = self
            .params
            .context
            .analysis_db
            .implicit_template_name(suffix)?
            .to_string();
        self.resolve_helper_call(&name, arg)
    }
}

impl BoundHelperValueResolver<'_, '_, '_, '_> {
    /// A call to a recognized custom merge helper resolves to the layered
    /// merge of its exact operands instead of the recursive body summary.
    ///
    /// The layer order is exact for the helper's full-overwrite keys; for
    /// other keys its per-kind exceptions (an empty-slice overwrite loses,
    /// boolean `or` sections) stay inside the accept direction because
    /// they surface only through Helm-FALSY overwrite values, which the
    /// truthy-scoped strict-operand walker never binds. The payload paths
    /// are marked YAML-serialized text so the conventional
    /// `include … | fromYaml` decode recovers the value.
    fn custom_merge_call(&mut self, name: &str, arg: Option<&TemplateExpr>) -> Option<EvalResult> {
        match self.params.context.analysis_db.custom_merge_helper(name)? {
            CustomMergeHelper::Pair => self.custom_pair_merge_call(arg),
            CustomMergeHelper::ParsedMapList => self.parsed_map_list_merge_call(arg),
        }
    }

    fn custom_pair_merge_call(&mut self, arg: Option<&TemplateExpr>) -> Option<EvalResult> {
        let TemplateExpr::Call { function, args } = arg?.deparen() else {
            return None;
        };
        if function != "list" || args.len() < 2 {
            return None;
        }
        let eval_operand = |expr: &TemplateExpr| {
            let mut seen = self.params.seen.clone();
            document_result_from_expr(
                expr,
                self.caller_env,
                self.params.outer,
                self.params.current_dot,
                self.params.context,
                &mut seen,
            )
        };
        let [input_expr, overwrite_expr, ..] = args.as_slice() else {
            return None;
        };
        let input = eval_operand(input_expr);
        let overwrite = eval_operand(overwrite_expr);
        let input_layer = input
            .value
            .clone()
            .map(|value| value.with_output_meta(&input.effects.local_output_meta))
            .and_then(AbstractValue::without_widened)
            .unwrap_or(AbstractValue::Unknown);
        let overwrite_layer = overwrite
            .value
            .clone()
            .map(|value| value.with_output_meta(&overwrite.effects.local_output_meta))
            .and_then(AbstractValue::without_widened)
            .unwrap_or(AbstractValue::Unknown);
        if input_layer.paths().is_empty() && overwrite_layer.paths().is_empty() {
            return None;
        }
        // A scrubbed identity inside a RANGE-member operand (a wildcard
        // path) keeps the scrub OUT of that layer: the ranged capture
        // machinery owns those member lanes, and the scrubbed identity
        // would displace the existential encodings its arms ride. The
        // OTHER operand keeps its scrub — airflow's per-worker-set merge
        // layers each `sets[]` member over the celery-scrubbed workers
        // base, and the base's layered typing must survive the per-set
        // round (the reroot chain reads the merged value back through
        // `.Values.workers`).
        let has_wildcard_path = |layer: &AbstractValue| {
            layer
                .paths()
                .iter()
                .any(|path| path.split('.').any(|segment| segment == "*"))
        };
        let input_layer = if has_wildcard_path(&input_layer) {
            input_layer.without_nil_scrub_markers()
        } else {
            input_layer
        };
        let overwrite_layer = if has_wildcard_path(&overwrite_layer) {
            overwrite_layer.without_nil_scrub_markers()
        } else {
            overwrite_layer
        };
        let value = AbstractValue::MergedLayers(vec![overwrite_layer, input_layer]);
        let mut effects = Effects::default();
        effects.merge(input.effects.execution_only());
        effects.merge(overwrite.effects.execution_only());
        let payload_paths = value.paths();
        effects
            .yaml_serialized_paths
            .extend(payload_paths.iter().cloned());
        effects.derived_text_paths.extend(payload_paths);
        Some(EvalResult::with_effects(Some(value), effects))
    }

    fn parsed_map_list_merge_call(&mut self, arg: Option<&TemplateExpr>) -> Option<EvalResult> {
        let TemplateExpr::Call {
            function: dict,
            args: entries,
        } = arg?.deparen()
        else {
            return None;
        };
        if dict != "dict" {
            return None;
        }
        let values = entries.chunks_exact(2).find_map(|entry| {
            matches!(
                entry.first().map(TemplateExpr::deparen),
                Some(TemplateExpr::Literal(helm_schema_ast::Literal::String(key)))
                    if key == "values"
            )
            .then(|| entry.get(1))
            .flatten()
        })?;
        let TemplateExpr::Call {
            function: list,
            args: operands,
        } = values.deparen()
        else {
            return None;
        };
        if list != "list" || operands.is_empty() {
            return None;
        }

        let mut effects = Effects::default();
        let mut layers = Vec::new();
        for operand in operands {
            let mut seen = self.params.seen.clone();
            let result = document_result_from_expr(
                operand,
                self.caller_env,
                self.params.outer,
                self.params.current_dot,
                self.params.context,
                &mut seen,
            );
            effects.merge(result.effects.execution_only());
            let layer = result.value?.without_widened()?;
            let path = layer.merge_layer_identity()?;
            if path.is_empty() {
                return None;
            }
            let mut meta = layer.output_meta().remove(&path).unwrap_or_default();
            meta.json_decoded = true;
            meta.parsed_map = true;
            meta.conjoin_branches(&std::collections::BTreeSet::from([
                helm_schema_core::Predicate::from(helm_schema_core::Guard::TypeIs {
                    path: path.clone(),
                    schema_type: "object".to_string(),
                }),
            ]));
            layers.push(AbstractValue::OutputPath(path, meta));
        }

        let value = AbstractValue::MergedLayers(layers);
        let payload_paths = value.paths();
        effects
            .yaml_serialized_paths
            .extend(payload_paths.iter().cloned());
        effects.derived_text_paths.extend(payload_paths);
        Some(EvalResult::with_effects(Some(value), effects))
    }

    /// A call to a recognized nil-scrub helper resolves to the operand's
    /// own identity with the scrubbed marker instead of the recursive
    /// body summary: the output IS the operand map minus its nil members,
    /// so member projection and layer ordering keep working while sink
    /// typing null-relaxes the scrubbed payload. The payload path is
    /// marked YAML-serialized text so the conventional
    /// `include … | fromYaml` decode passes the value through.
    fn nil_scrub_call(&mut self, name: &str, arg: Option<&TemplateExpr>) -> Option<EvalResult> {
        self.params.context.analysis_db.nil_scrub_helper(name)?;
        let arg = arg?;
        let mut seen = self.params.seen.clone();
        let operand = document_result_from_expr(
            arg,
            self.caller_env,
            self.params.outer,
            self.params.current_dot,
            self.params.context,
            &mut seen,
        );
        let (path, mut meta) = match operand.value.as_ref()?.clone().without_widened()? {
            AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path)
                if !path.is_empty() =>
            {
                (path, crate::helper_meta::HelperOutputMeta::default())
            }
            AbstractValue::OutputPath(path, meta) if meta.json_decoded && !path.is_empty() => {
                (path, meta)
            }
            _ => return None,
        };
        meta.json_decoded = true;
        meta.nil_scrubbed = true;
        let value = AbstractValue::OutputPath(path.clone(), meta);
        let mut effects = Effects::default();
        effects.merge(operand.effects.execution_only());
        effects.yaml_serialized_paths.insert(path.clone());
        effects.derived_text_paths.insert(path);
        Some(EvalResult::with_effects(Some(value), effects))
    }
}
