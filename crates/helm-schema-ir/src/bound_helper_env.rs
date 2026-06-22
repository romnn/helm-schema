use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::fragment_expr_eval::{
    FragmentEvalContext, fragment_value_from_expr, helper_value_from_expr_with_fragment_locals,
};
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::template_expr_analysis::expr_contains_helper_call;

pub(crate) struct BoundHelperEnv<'bindings, 'context> {
    bindings: &'bindings HashMap<String, AbstractValue>,
    current_dot: Option<&'bindings AbstractValue>,
    context: FragmentEvalContext<'context>,
}

impl<'bindings, 'context> BoundHelperEnv<'bindings, 'context> {
    pub(crate) fn new(
        bindings: &'bindings HashMap<String, AbstractValue>,
        current_dot: Option<&'bindings AbstractValue>,
        context: FragmentEvalContext<'context>,
    ) -> Self {
        Self {
            bindings,
            current_dot,
            context,
        }
    }

    pub(crate) fn current_dot_fragment(&self) -> Option<AbstractValue> {
        self.current_dot.map(AbstractValue::to_context_value)
    }

    pub(crate) fn external_default_fallback_paths_in_exprs(
        &self,
        exprs: &[TemplateExpr],
    ) -> BTreeSet<String> {
        let env = EvalEnv::from_helper_context(Some(self.bindings), self.current_dot);
        let mut paths = BTreeSet::new();
        for expr in exprs {
            paths.extend(eval_expr(expr, &env).effects.defaults);
        }
        paths
    }

    pub(crate) fn type_hints_in_exprs(
        &self,
        exprs: &[TemplateExpr],
        local_bindings: &HashMap<String, AbstractValue>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        let env = EvalEnv::from_helper_context_with_fragment_locals(
            Some(self.bindings),
            self.current_dot,
            local_bindings,
        );
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for expr in exprs {
            for (path, hints) in eval_expr(expr, &env).effects.schema_type_hints() {
                out.entry(path).or_default().extend(hints);
            }
        }
        out
    }

    pub(crate) fn summarize_calls_in_exprs(
        &self,
        exprs: &[TemplateExpr],
        local_bindings: &HashMap<String, AbstractValue>,
        seen: &mut HashSet<String>,
    ) -> HelperSummary {
        self.context
            .helper_summaries()
            .summarize_bound_helper_calls_in_exprs(
                exprs,
                Some(self.bindings),
                self.current_dot,
                local_bindings,
                self.context,
                seen,
            )
    }

    pub(crate) fn helper_value_from_expr(
        &self,
        expr: &TemplateExpr,
        local_bindings: &HashMap<String, AbstractValue>,
        seen: &mut HashSet<String>,
    ) -> Option<AbstractValue> {
        helper_value_from_expr_with_fragment_locals(
            expr,
            local_bindings,
            Some(self.bindings),
            self.current_dot,
            self.context,
            seen,
        )
    }

    pub(crate) fn fragment_value_from_expr(
        &self,
        expr: &TemplateExpr,
        local_bindings: &HashMap<String, AbstractValue>,
        seen: &mut HashSet<String>,
    ) -> Option<AbstractValue> {
        let current_dot_fragment = self.current_dot_fragment();
        if !expr_contains_helper_call(expr)
            && let Some(binding) = fragment_value_from_expr(
                expr,
                local_bindings,
                current_dot_fragment.as_ref(),
                self.context,
                seen,
            )
        {
            return Some(binding);
        }
        if let Some(binding) = self.helper_value_from_expr(expr, local_bindings, seen) {
            return Some(binding.to_context_value());
        }
        fragment_value_from_expr(
            expr,
            local_bindings,
            current_dot_fragment.as_ref(),
            self.context,
            seen,
        )
    }

    pub(crate) fn output_meta_from_exprs(
        &self,
        exprs: &[TemplateExpr],
        local_bindings: &HashMap<String, AbstractValue>,
        seen_seed: &HashSet<String>,
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        let mut seen = seen_seed.clone();
        for expr in exprs {
            if let Some(binding) = self.helper_value_from_expr(expr, local_bindings, &mut seen) {
                for (path, meta) in binding.output_meta() {
                    out.entry(path).or_default().merge(meta);
                }
            }
        }
        out
    }

    pub(crate) fn string_outputs_from_exprs(
        &self,
        exprs: &[TemplateExpr],
        local_bindings: &HashMap<String, AbstractValue>,
        seen_seed: &HashSet<String>,
    ) -> BTreeSet<String> {
        let mut strings = BTreeSet::new();
        let mut seen = seen_seed.clone();
        for expr in exprs {
            if let Some(binding) = self.helper_value_from_expr(expr, local_bindings, &mut seen) {
                strings.extend(binding.strings());
                continue;
            }
            if let Some(binding) = self.fragment_value_from_expr(expr, local_bindings, &mut seen) {
                strings.extend(binding.strings());
            }
        }
        strings
    }
}
