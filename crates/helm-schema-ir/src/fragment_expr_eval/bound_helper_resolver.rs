use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};

use super::FragmentEvalContext;

pub(super) fn eval_expr_result_with_bound_helpers(
    expr: &TemplateExpr,
    env: &EvalEnv,
    params: BoundHelperValueResolverParams<'_, '_, '_>,
) -> EvalResult {
    let mut resolver = BoundHelperValueResolver { params };
    eval_expr_with_helper_calls(expr, env, &mut resolver)
}

pub(super) struct BoundHelperValueResolverParams<'a, 'context, 'seen> {
    pub(super) fragment_locals: &'a HashMap<String, AbstractValue>,
    pub(super) outer: Option<&'a HashMap<String, AbstractValue>>,
    pub(super) current_dot: Option<&'a AbstractValue>,
    pub(super) context: FragmentEvalContext<'context>,
    pub(super) seen: &'seen mut HashSet<String>,
}

struct BoundHelperValueResolver<'a, 'context, 'seen> {
    params: BoundHelperValueResolverParams<'a, 'context, 'seen>,
}

impl HelperCallValueResolver for BoundHelperValueResolver<'_, '_, '_> {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        arg: Option<&TemplateExpr>,
    ) -> Option<EvalResult> {
        if !self.params.context.analysis_db.has_helper(name) {
            return None;
        }
        if self.params.seen.contains(name) {
            return Some(EvalResult::none());
        }
        let summary = self.params.context.analysis_db.summarize_bound_helper_call(
            name,
            arg,
            self.params.outer,
            self.params.current_dot,
            self.params.fragment_locals,
            self.params.context,
            self.params.seen,
        );
        // The resolver boundary is the one place summary facts enter
        // expression effects; collectors read the Effects fields only.
        // Encoded rows surface as encoded paths so value-lattice lowerings
        // keep the "sink does not constrain the value" semantics the row
        // recorded (the projected value's output paths carry no encoding
        // flag).
        let effects = Effects {
            chart_default_paths: summary.chart_defaults.clone(),
            type_hints: summary.type_hints.clone(),
            guarded_type_hints: summary.guarded_type_hints.clone(),
            parsed_yaml_input_paths: summary.parsed_yaml_input_paths.clone(),
            yaml_serialized_paths: summary.yaml_serialized_paths.clone(),
            encoded_paths: summary.encoded_paths(),
            shape_erased_paths: summary.shape_erased_paths.clone(),
            string_contract_paths: summary.string_contract_paths.clone(),
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
            ..Effects::default()
        };
        Some(EvalResult::with_effects(summary.value.clone(), effects))
    }
}
